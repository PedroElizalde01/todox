pub mod app;
pub mod cli;
pub mod convert;
pub mod model;
pub mod repository;
pub mod tui;
pub mod ui;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use notify::{Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

use app::App;
use cli::{Cli, Command};
use repository::{find_root, load_dir};
use tui::Tui;
use ui::draw;

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::JsonToon(args)) => return convert::run(convert::Direction::JsonToToon, args),
        Some(Command::ToonJson(args)) => return convert::run(convert::Direction::ToonToJson, args),
        None => {}
    }

    let root = resolve_root(cli.path)?;
    let tickets = load_dir(&root)?;
    if tickets.is_empty() {
        eprintln!("no tickets in {}", root.display());
        std::process::exit(0);
    }

    let mut tui = Tui::enter()?;
    let (watcher, rx) = if cli.no_watch {
        (None, None)
    } else {
        let (watcher, rx) = build_watcher(&root)?;
        (Some(watcher), Some(rx))
    };

    let result = run_loop(&mut tui, App::new(tickets), &root, rx.as_ref());
    drop(watcher);
    result
}

fn resolve_root(path: Option<PathBuf>) -> Result<PathBuf> {
    let start = path.unwrap_or(std::env::current_dir()?);
    if start.is_dir()
        && start
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, ".todo" | "todo"))
    {
        return Ok(start);
    }

    match find_root(&start) {
        Some(root) => Ok(root),
        None => {
            eprintln!("no .todo or todo folder found in {}", start.display());
            std::process::exit(1);
        }
    }
}

fn build_watcher(root: &Path) -> Result<(RecommendedWatcher, mpsc::Receiver<NotifyEvent>)> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<NotifyEvent>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok((watcher, rx))
}

fn run_loop(
    tui: &mut Tui,
    mut app: App,
    root: &Path,
    rx: Option<&mpsc::Receiver<NotifyEvent>>,
) -> Result<()> {
    loop {
        tui.draw(|frame| draw(frame, &mut app))?;

        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
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

        if let Some(rx) = rx {
            reload_if_dirty(&mut app, root, rx);
        }
    }
}

fn reload_if_dirty(app: &mut App, root: &Path, rx: &mpsc::Receiver<NotifyEvent>) {
    let mut dirty = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            dirty = true;
        }
    }

    if dirty {
        thread::sleep(Duration::from_millis(50));
        while rx.try_recv().is_ok() {}
        let _ = app.reload(root);
    }
}
