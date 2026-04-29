use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{app::App, model::Section};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    frame.render_widget(Paragraph::new(breadcrumbs(app)), chunks[0]);

    if app.detail {
        draw_detail(frame, app, chunks[1]);
    } else {
        draw_list(frame, app, chunks[1]);
    }

    frame.render_widget(help(), chunks[2]);
}

fn breadcrumbs(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (_, _, label)) in app.stack.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" › ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            label.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    if app.detail {
        if let Some(ticket) = app.selected() {
            spans.push(Span::styled(" › ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                ticket.title.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }
    Line::from(spans)
}

fn help() -> Paragraph<'static> {
    Paragraph::new(Line::from(vec![
        Span::styled("↑↓/jk", Style::default().fg(Color::Cyan)),
        Span::raw(" move  "),
        Span::styled("→/l/⏎", Style::default().fg(Color::Cyan)),
        Span::raw(" open  "),
        Span::styled("←/h/⌫", Style::default().fg(Color::Cyan)),
        Span::raw(" back  "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]))
    .style(Style::default().fg(Color::DarkGray))
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let (tickets, state, label) = app.cur_mut();
    let items: Vec<ListItem> = tickets.iter().map(ticket_list_item).collect();

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

    frame.render_stateful_widget(list, area, state);
}

fn ticket_list_item(ticket: &crate::model::Ticket) -> ListItem<'static> {
    let (glyph, color) = status_glyph(&ticket.raw.status);
    let done = is_done(&ticket.raw.status);
    let mut title_style = Style::default().fg(Color::White);
    if done {
        title_style = title_style
            .add_modifier(Modifier::CROSSED_OUT)
            .add_modifier(Modifier::DIM)
            .fg(Color::DarkGray);
    }

    let mut spans = vec![
        Span::styled(
            format!(" {} ", glyph),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(ticket.title.clone(), title_style),
    ];

    if !ticket.raw.priority.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("[{}]", ticket.raw.priority),
            Style::default().fg(priority_color(&ticket.raw.priority)),
        ));
    }
    if !ticket.raw.estimate.is_empty() {
        spans.push(Span::styled(
            format!(" {}", ticket.raw.estimate),
            Style::default().fg(Color::Magenta),
        ));
    }
    if !ticket.children.is_empty() {
        spans.push(Span::styled(
            format!("  ▸ {}", ticket.children.len()),
            Style::default().fg(Color::Blue),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn draw_detail(frame: &mut Frame, app: &mut App, area: Rect) {
    let Some(ticket) = app.selected().cloned() else {
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    let (glyph, color) = status_glyph(&ticket.raw.status);
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", glyph),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            ticket.title.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    let meta = detail_meta(&ticket);
    if !meta.spans.is_empty() {
        lines.push(meta);
    }

    lines.push(Line::from(""));

    if !ticket.raw.description.is_empty() {
        for line in ticket.raw.description.lines() {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Color::Gray),
            )));
        }
        lines.push(Line::from(""));
    }

    for section in &ticket.raw.sections {
        if !section.subtitle.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                format!("▍ {}", section.subtitle),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]));
        }
        render_section(&mut lines, section);
        lines.push(Line::from(""));
    }

    if !ticket.children.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("▸ {} subtickets — press → to open", ticket.children.len()),
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(format!(" {} ", ticket.title)),
    );

    frame.render_widget(paragraph, area);
}

fn detail_meta(ticket: &crate::model::Ticket) -> Line<'static> {
    let (.., status_color) = status_glyph(&ticket.raw.status);
    let mut meta = Vec::new();

    if !ticket.raw.status.is_empty() {
        meta.push(Span::styled(
            "status: ",
            Style::default().fg(Color::DarkGray),
        ));
        meta.push(Span::styled(
            ticket.raw.status.clone(),
            Style::default().fg(status_color),
        ));
        meta.push(Span::raw("  "));
    }
    if !ticket.raw.priority.is_empty() {
        meta.push(Span::styled(
            "priority: ",
            Style::default().fg(Color::DarkGray),
        ));
        meta.push(Span::styled(
            ticket.raw.priority.clone(),
            Style::default().fg(priority_color(&ticket.raw.priority)),
        ));
        meta.push(Span::raw("  "));
    }
    if !ticket.raw.estimate.is_empty() {
        meta.push(Span::styled(
            "estimate: ",
            Style::default().fg(Color::DarkGray),
        ));
        meta.push(Span::styled(
            ticket.raw.estimate.clone(),
            Style::default().fg(Color::Magenta),
        ));
    }

    Line::from(meta)
}

fn render_section(lines: &mut Vec<Line<'static>>, section: &Section) {
    use serde_json::Value;

    let kind = section.kind.as_deref().unwrap_or("").to_lowercase();
    match &section.content {
        Value::String(text) => {
            for line in text.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {}", line),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
        Value::Array(items) => {
            let numbered = matches!(kind.as_str(), "numbered" | "ordered");
            for (index, item) in items.iter().enumerate() {
                match item {
                    Value::String(text) => lines.push(section_text_item(text, numbered, index)),
                    Value::Object(map) => lines.push(section_check_item(map)),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn section_text_item(text: &str, numbered: bool, index: usize) -> Line<'static> {
    let bullet = if numbered {
        format!("  {}. ", index + 1)
    } else {
        "  • ".into()
    };

    Line::from(vec![
        Span::styled(bullet, Style::default().fg(Color::Yellow)),
        Span::styled(text.to_string(), Style::default().fg(Color::White)),
    ])
}

fn section_check_item(map: &serde_json::Map<String, serde_json::Value>) -> Line<'static> {
    let checked = map
        .get("checked")
        .or_else(|| map.get("done"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let text = map
        .get("text")
        .or_else(|| map.get("title"))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    let (mark, color) = if checked {
        ("  ☒ ", Color::Green)
    } else {
        ("  ☐ ", Color::Gray)
    };

    let mut text_style = Style::default().fg(Color::White);
    if checked {
        text_style = text_style
            .add_modifier(Modifier::CROSSED_OUT)
            .fg(Color::DarkGray);
    }

    Line::from(vec![
        Span::styled(
            mark,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(text, text_style),
    ])
}

fn status_glyph(status: &str) -> (&'static str, Color) {
    match status.to_lowercase().as_str() {
        "done" | "complete" | "completed" | "closed" => ("●", Color::Green),
        "doing" | "in_progress" | "in-progress" | "wip" | "active" => ("◐", Color::Yellow),
        "blocked" | "block" => ("⊘", Color::Red),
        "review" | "in_review" => ("◑", Color::Magenta),
        "todo" | "open" | "" => ("○", Color::Gray),
        _ => ("◇", Color::Cyan),
    }
}

fn priority_color(priority: &str) -> Color {
    match priority.to_lowercase().as_str() {
        "critical" | "urgent" | "p0" => Color::Red,
        "high" | "p1" => Color::LightRed,
        "medium" | "med" | "p2" => Color::Yellow,
        "low" | "p3" => Color::Blue,
        _ => Color::DarkGray,
    }
}

fn is_done(status: &str) -> bool {
    matches!(
        status.to_lowercase().as_str(),
        "done" | "complete" | "completed" | "closed"
    )
}
