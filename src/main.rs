use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use serde::Deserialize;
use notify::{RecursiveMode, Watcher};
use std::{
    fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(untagged)]
enum SectionContent {
    Text(String),
    Items(Vec<ItemEntry>),
    #[default]
    Empty,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum ItemEntry {
    Plain(String),
    Check { #[serde(default)] checked: bool, text: String },
}

#[derive(Debug, Deserialize, Clone, Default)]
struct Section {
    #[serde(default)]
    subtitle: String,
    #[serde(default, alias = "type")]
    kind: Option<String>, // text|items|numbered|checks
    #[serde(default)]
    content: serde_yaml::Value,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct TicketRaw {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    priority: String,
    #[serde(default, alias = "estimate", alias = "estimate_time", alias = "estimateTime")]
    estimate: String,
    #[serde(default)]
    sections: Vec<Section>,
}

#[derive(Debug, Clone)]
struct Ticket {
    title: String,
    path: PathBuf,
    raw: TicketRaw,
    children: Vec<Ticket>,
}

fn find_root(start: &Path) -> Option<PathBuf> {
    for name in [".todo", "todo"] {
        let p = start.join(name);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn load_dir(dir: &Path) -> Result<Vec<Ticket>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    // Map: stem -> (yml_path?, child_dir?)
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (Option<PathBuf>, Option<PathBuf>)> = BTreeMap::new();
    for e in entries {
        let path = e.path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            map.entry(name).or_default().1 = Some(path);
        } else if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml") | Some("yaml")
        ) {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            map.entry(stem).or_default().0 = Some(path);
        }
    }

    for (stem, (yml, sub)) in map {
        let raw: TicketRaw = if let Some(y) = &yml {
            let txt = fs::read_to_string(y).with_context(|| format!("read {:?}", y))?;
            serde_yaml::from_str(&txt).unwrap_or_default()
        } else if let Some(d) = &sub {
            // folder-only: try index.yml/.yaml
            let mut r = TicketRaw::default();
            for n in ["index.yml", "index.yaml", "_.yml", "_.yaml"] {
                let p = d.join(n);
                if p.is_file() {
                    if let Ok(txt) = fs::read_to_string(&p) {
                        r = serde_yaml::from_str(&txt).unwrap_or_default();
                    }
                    break;
                }
            }
            r
        } else {
            TicketRaw::default()
        };

        let title = raw
            .title
            .clone()
            .or_else(|| raw.name.clone())
            .unwrap_or_else(|| stem.clone());
        let children = if let Some(d) = &sub {
            load_dir(d)?
        } else {
            Vec::new()
        };
        let path = yml.clone().or(sub.clone()).unwrap_or_default();
        out.push(Ticket {
            title,
            path,
            raw,
            children,
        });
    }
    Ok(out)
}

fn status_glyph(s: &str) -> (&'static str, Color) {
    match s.to_lowercase().as_str() {
        "done" | "complete" | "completed" | "closed" => ("●", Color::Green),
        "doing" | "in_progress" | "in-progress" | "wip" | "active" => ("◐", Color::Yellow),
        "blocked" | "block" => ("⊘", Color::Red),
        "review" | "in_review" => ("◑", Color::Magenta),
        "todo" | "open" | "" => ("○", Color::Gray),
        _ => ("◇", Color::Cyan),
    }
}

fn priority_color(p: &str) -> Color {
    match p.to_lowercase().as_str() {
        "critical" | "urgent" | "p0" => Color::Red,
        "high" | "p1" => Color::LightRed,
        "medium" | "med" | "p2" => Color::Yellow,
        "low" | "p3" => Color::Blue,
        _ => Color::DarkGray,
    }
}

struct App {
    stack: Vec<(Vec<Ticket>, ListState, String)>, // tickets, state, label
    detail: bool,
}

impl App {
    fn new(root: Vec<Ticket>) -> Self {
        let mut s = ListState::default();
        if !root.is_empty() {
            s.select(Some(0));
        }
        Self {
            stack: vec![(root, s, "todo".into())],
            detail: false,
        }
    }
    fn cur(&self) -> &(Vec<Ticket>, ListState, String) {
        self.stack.last().unwrap()
    }
    fn cur_mut(&mut self) -> &mut (Vec<Ticket>, ListState, String) {
        self.stack.last_mut().unwrap()
    }
    fn selected(&self) -> Option<&Ticket> {
        let (v, s, _) = self.cur();
        s.selected().and_then(|i| v.get(i))
    }
    fn move_sel(&mut self, d: i32) {
        let (v, s, _) = self.cur_mut();
        if v.is_empty() {
            return;
        }
        let i = s.selected().unwrap_or(0) as i32 + d;
        let i = i.rem_euclid(v.len() as i32) as usize;
        s.select(Some(i));
    }
    fn enter(&mut self) {
        if self.detail {
            // already viewing detail; if has children, drill into them
            if let Some(t) = self.selected() {
                if !t.children.is_empty() {
                    let kids = t.children.clone();
                    let label = t.title.clone();
                    let mut st = ListState::default();
                    st.select(Some(0));
                    self.stack.push((kids, st, label));
                    self.detail = false;
                }
            }
            return;
        }
        if let Some(t) = self.selected() {
            // open detail view
            if t.children.is_empty() && t.raw_is_empty() {
                // nothing to show, but show anyway
            }
            self.detail = true;
        }
    }
    fn snapshot(&self) -> Vec<String> {
        let mut path = Vec::new();
        for (v, s, _) in &self.stack {
            if let Some(i) = s.selected() {
                if let Some(t) = v.get(i) {
                    path.push(t.title.clone());
                }
            }
        }
        path
    }
    fn reload(&mut self, root: &Path) -> Result<()> {
        let path = self.snapshot();
        let detail = self.detail;
        let tickets = load_dir(root)?;
        self.stack.clear();
        let mut s = ListState::default();
        if !tickets.is_empty() {
            s.select(Some(0));
        }
        self.stack.push((tickets, s, "todo".into()));
        for i in 0..path.len() {
            let title = path[i].clone();
            let (v, s, _) = self.stack.last_mut().unwrap();
            let Some(idx) = v.iter().position(|t| t.title == title) else {
                break;
            };
            s.select(Some(idx));
            if i + 1 < path.len() {
                let kids = v[idx].children.clone();
                if kids.is_empty() {
                    break;
                }
                let label = v[idx].title.clone();
                let mut ns = ListState::default();
                ns.select(Some(0));
                self.stack.push((kids, ns, label));
            }
        }
        self.detail = detail;
        Ok(())
    }
    fn back(&mut self) {
        if self.detail {
            self.detail = false;
            return;
        }
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
}

impl Ticket {
    fn raw_is_empty(&self) -> bool {
        self.raw.description.is_empty() && self.raw.sections.is_empty()
    }
}

fn main() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = match find_root(&cwd) {
        Some(p) => p,
        None => {
            eprintln!("no .todo or todo folder found in {}", cwd.display());
            std::process::exit(1);
        }
    };
    let tickets = load_dir(&root)?;
    if tickets.is_empty() {
        eprintln!("no tickets in {}", root.display());
        std::process::exit(0);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            let _ = tx.send(ev);
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let res = run(&mut term, App::new(tickets), &root, rx);

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    res
}

fn run(
    term: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
    root: &Path,
    rx: mpsc::Receiver<notify::Event>,
) -> Result<()> {
    loop {
        term.draw(|f| draw(f, &mut app))?;

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if app.detail || app.stack.len() > 1 {
                            app.back();
                        } else {
                            return Ok(());
                        }
                    }
                    KeyCode::Char('Q') => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => app.move_sel(1),
                    KeyCode::Up | KeyCode::Char('k') => app.move_sel(-1),
                    KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => app.enter(),
                    KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => app.back(),
                    KeyCode::Char('r') => {
                        let _ = app.reload(root);
                    }
                    _ => {}
                }
            }
        }

        let mut dirty = false;
        while let Ok(ev) = rx.try_recv() {
            use notify::EventKind::*;
            if matches!(ev.kind, Create(_) | Modify(_) | Remove(_)) {
                dirty = true;
            }
        }
        if dirty {
            // small debounce: drain bursts
            std::thread::sleep(Duration::from_millis(50));
            while rx.try_recv().is_ok() {}
            let _ = app.reload(root);
        }
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    // breadcrumbs
    let crumbs: Vec<Span> = {
        let mut v = Vec::new();
        for (i, (_, _, lbl)) in app.stack.iter().enumerate() {
            if i > 0 {
                v.push(Span::styled(" › ", Style::default().fg(Color::DarkGray)));
            }
            v.push(Span::styled(
                lbl.clone(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
        }
        if app.detail {
            if let Some(t) = app.selected() {
                v.push(Span::styled(" › ", Style::default().fg(Color::DarkGray)));
                v.push(Span::styled(
                    t.title.clone(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ));
            }
        }
        v
    };
    f.render_widget(Paragraph::new(Line::from(crumbs)), chunks[0]);

    if app.detail {
        draw_detail(f, app, chunks[1]);
    } else {
        draw_list(f, app, chunks[1]);
    }

    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑↓/jk", Style::default().fg(Color::Cyan)),
        Span::raw(" move  "),
        Span::styled("→/l/⏎", Style::default().fg(Color::Cyan)),
        Span::raw(" open  "),
        Span::styled("←/h/⌫", Style::default().fg(Color::Cyan)),
        Span::raw(" back  "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, chunks[2]);
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let (tickets, state, label) = app.cur_mut();
    let items: Vec<ListItem> = tickets
        .iter()
        .map(|t| {
            let (g, c) = status_glyph(&t.raw.status);
            let done = matches!(
                t.raw.status.to_lowercase().as_str(),
                "done" | "complete" | "completed" | "closed"
            );
            let mut title_style = Style::default().fg(Color::White);
            if done {
                title_style = title_style
                    .add_modifier(Modifier::CROSSED_OUT)
                    .add_modifier(Modifier::DIM)
                    .fg(Color::DarkGray);
            }
            let mut spans = vec![
                Span::styled(format!(" {} ", g), Style::default().fg(c).add_modifier(Modifier::BOLD)),
                Span::styled(t.title.clone(), title_style),
            ];
            if !t.raw.priority.is_empty() {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}]", t.raw.priority),
                    Style::default().fg(priority_color(&t.raw.priority)),
                ));
            }
            if !t.raw.estimate.is_empty() {
                spans.push(Span::styled(
                    format!(" {}", t.raw.estimate),
                    Style::default().fg(Color::Magenta),
                ));
            }
            if !t.children.is_empty() {
                spans.push(Span::styled(
                    format!("  ▸ {}", t.children.len()),
                    Style::default().fg(Color::Blue),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(format!(" {} ", label)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 60))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, state);
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let t = match app.selected() {
        Some(t) => t.clone(),
        None => return,
    };
    let mut lines: Vec<Line> = Vec::new();
    let (g, c) = status_glyph(&t.raw.status);
    lines.push(Line::from(vec![
        Span::styled(format!("{} ", g), Style::default().fg(c).add_modifier(Modifier::BOLD)),
        Span::styled(
            t.title.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]));
    let mut meta: Vec<Span> = Vec::new();
    if !t.raw.status.is_empty() {
        meta.push(Span::styled("status: ", Style::default().fg(Color::DarkGray)));
        meta.push(Span::styled(t.raw.status.clone(), Style::default().fg(c)));
        meta.push(Span::raw("  "));
    }
    if !t.raw.priority.is_empty() {
        meta.push(Span::styled("priority: ", Style::default().fg(Color::DarkGray)));
        meta.push(Span::styled(
            t.raw.priority.clone(),
            Style::default().fg(priority_color(&t.raw.priority)),
        ));
        meta.push(Span::raw("  "));
    }
    if !t.raw.estimate.is_empty() {
        meta.push(Span::styled("estimate: ", Style::default().fg(Color::DarkGray)));
        meta.push(Span::styled(
            t.raw.estimate.clone(),
            Style::default().fg(Color::Magenta),
        ));
    }
    if !meta.is_empty() {
        lines.push(Line::from(meta));
    }
    lines.push(Line::from(""));
    if !t.raw.description.is_empty() {
        for l in t.raw.description.lines() {
            lines.push(Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Gray),
            )));
        }
        lines.push(Line::from(""));
    }
    for sec in &t.raw.sections {
        if !sec.subtitle.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                format!("▍ {}", sec.subtitle),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )]));
        }
        render_section(&mut lines, sec);
        lines.push(Line::from(""));
    }
    if !t.children.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("▸ {} subtickets — press → to open", t.children.len()),
            Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC),
        )));
    }

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {} ", t.title)),
        );
    f.render_widget(p, area);
}

fn render_section(lines: &mut Vec<Line>, sec: &Section) {
    use serde_yaml::Value;
    let kind = sec.kind.as_deref().unwrap_or("").to_lowercase();
    match &sec.content {
        Value::String(s) => {
            for l in s.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", l),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
        Value::Sequence(seq) => {
            let numbered = kind == "numbered" || kind == "ordered";
            for (i, item) in seq.iter().enumerate() {
                match item {
                    Value::String(s) => {
                        let bullet = if numbered {
                            format!("  {}. ", i + 1)
                        } else {
                            "  • ".into()
                        };
                        lines.push(Line::from(vec![
                            Span::styled(bullet, Style::default().fg(Color::Yellow)),
                            Span::styled(s.clone(), Style::default().fg(Color::White)),
                        ]));
                    }
                    Value::Mapping(m) => {
                        // checked item: {checked: bool, text: ...} or {text: ..., done: bool}
                        let checked = m
                            .get(Value::String("checked".into()))
                            .or_else(|| m.get(Value::String("done".into())))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let text = m
                            .get(Value::String("text".into()))
                            .or_else(|| m.get(Value::String("title".into())))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let (mark, color) = if checked {
                            ("  ☒ ", Color::Green)
                        } else {
                            ("  ☐ ", Color::Gray)
                        };
                        let mut st = Style::default().fg(Color::White);
                        if checked {
                            st = st.add_modifier(Modifier::CROSSED_OUT).fg(Color::DarkGray);
                        }
                        lines.push(Line::from(vec![
                            Span::styled(mark, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                            Span::styled(text, st),
                        ]));
                    }
                    _ => {}
                }
            }
        }
        Value::Null => {}
        _ => {}
    }
    // unused
    let _ = SectionContent::Empty;
    let _ = ItemEntry::Plain(String::new());
}
