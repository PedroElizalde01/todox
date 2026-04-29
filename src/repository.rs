use anyhow::{Context, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::model::{Ticket, TicketRaw};

/// Source format of a ticket file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TicketFormat {
    Toon,
    Json,
}

impl TicketFormat {
    /// Map a filesystem extension to a ticket format. Case-insensitive.
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "toon" => Some(Self::Toon),
            "json" => Some(Self::Json),
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }
}

/// Parse a TOON ticket. Returns a default `TicketRaw` on error so the UI stays
/// resilient to malformed files. Use [`parse_ticket_str`] when error context matters.
#[must_use]
pub fn parse_toon(text: &str) -> TicketRaw {
    parse_ticket_str(text, TicketFormat::Toon).unwrap_or_default()
}

/// Parse a JSON ticket. Same resilience contract as [`parse_toon`].
#[must_use]
pub fn parse_json(text: &str) -> TicketRaw {
    parse_ticket_str(text, TicketFormat::Json).unwrap_or_default()
}

fn parse_ticket_str(text: &str, format: TicketFormat) -> Result<TicketRaw> {
    let value = match format {
        TicketFormat::Toon => toon_format::decode(text, &toon_format::DecodeOptions::default())
            .map_err(|e| anyhow::anyhow!("toon decode: {e}"))?,
        TicketFormat::Json => serde_json::from_str(text).context("json decode")?,
    };
    serde_json::from_value(value).context("ticket schema mismatch")
}

#[must_use]
pub fn find_root(start: &Path) -> Option<PathBuf> {
    [".todo", "todo"]
        .into_iter()
        .map(|name| start.join(name))
        .find(|path| path.is_dir())
}

pub fn load_dir(dir: &Path) -> Result<Vec<Ticket>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    // Per stem: track an optional ticket file (with its format) and an optional child directory.
    type Group = (Option<(PathBuf, TicketFormat)>, Option<PathBuf>);
    let mut grouped: BTreeMap<String, Group> = BTreeMap::new();

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            grouped.entry(name).or_default().1 = Some(path);
            continue;
        }

        let Some(format) = TicketFormat::from_path(&path) else {
            continue;
        };
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        let slot = &mut grouped.entry(stem.clone()).or_default().0;
        match slot {
            None => *slot = Some((path, format)),
            Some((existing, existing_format)) => {
                // Two files share a stem (e.g. `foo.toon` + `foo.json`).
                // Prefer TOON as the canonical format and warn so the conflict is visible.
                if *existing_format != TicketFormat::Toon && format == TicketFormat::Toon {
                    eprintln!(
                        "ticket conflict: preferring {} over {}",
                        path.display(),
                        existing.display()
                    );
                    *slot = Some((path, format));
                } else {
                    eprintln!(
                        "ticket conflict: ignoring {} (already loaded {})",
                        path.display(),
                        existing.display()
                    );
                }
            }
        }
    }

    grouped
        .into_iter()
        .map(|(stem, (ticket_file, child_dir))| load_ticket(stem, ticket_file, child_dir))
        .collect()
}

fn load_ticket(
    stem: String,
    ticket_file: Option<(PathBuf, TicketFormat)>,
    child_dir: Option<PathBuf>,
) -> Result<Ticket> {
    let raw = match (&ticket_file, &child_dir) {
        (Some((path, format)), _) => read_ticket_file(path, *format)?,
        (None, Some(dir)) => read_directory_index(dir),
        (None, None) => TicketRaw::default(),
    };

    let title = raw
        .title
        .clone()
        .or_else(|| raw.name.clone())
        .unwrap_or(stem);
    let children = match &child_dir {
        Some(dir) => load_dir(dir)?,
        None => Vec::new(),
    };
    let path = ticket_file
        .map(|(p, _)| p)
        .or(child_dir)
        .unwrap_or_default();

    Ok(Ticket {
        title,
        path,
        raw,
        children,
    })
}

fn read_ticket_file(path: &Path, format: TicketFormat) -> Result<TicketRaw> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    match parse_ticket_str(&text, format) {
        Ok(raw) => Ok(raw),
        Err(error) => {
            eprintln!("parse {}: {error:#}", path.display());
            Ok(TicketRaw::default())
        }
    }
}

/// Look for an index ticket inside `dir`. Tries TOON first, then JSON, under
/// the canonical names `index` and `_`.
fn read_directory_index(dir: &Path) -> TicketRaw {
    const NAMES: &[&str] = &["index", "_"];
    const FORMATS: &[(&str, TicketFormat)] =
        &[("toon", TicketFormat::Toon), ("json", TicketFormat::Json)];

    for name in NAMES {
        for (ext, format) in FORMATS {
            let path = dir.join(format!("{name}.{ext}"));
            if !path.is_file() {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(text) => match parse_ticket_str(&text, *format) {
                    Ok(raw) => return raw,
                    Err(error) => {
                        eprintln!("parse {}: {error:#}", path.display());
                    }
                },
                Err(error) => {
                    eprintln!("read {}: {error}", path.display());
                }
            }
        }
    }

    TicketRaw::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("todox-test-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_toon_is_resilient_on_invalid_input() {
        let raw = parse_toon("not valid toon");
        assert!(raw.title.is_none());
        assert!(raw.description.is_empty());
        assert!(raw.sections.is_empty());
    }

    #[test]
    fn parse_json_is_resilient_on_invalid_input() {
        let raw = parse_json("not valid json");
        assert!(raw.title.is_none());
        assert!(raw.sections.is_empty());
    }

    #[test]
    fn parse_json_decodes_ticket_schema() {
        let raw = parse_json(
            r#"{"title":"X","status":"todo","priority":"high","sections":[{"subtitle":"S","content":"hi"}]}"#,
        );
        assert_eq!(raw.title.as_deref(), Some("X"));
        assert_eq!(raw.status, "todo");
        assert_eq!(raw.priority, "high");
        assert_eq!(raw.sections.len(), 1);
        assert_eq!(raw.sections[0].subtitle, "S");
    }

    #[test]
    fn find_root_prefers_dot_todo_then_todo() {
        let root = temp_dir();
        let hidden = root.join(".todo");
        let plain = root.join("todo");
        fs::create_dir_all(&hidden).unwrap();
        fs::create_dir_all(&plain).unwrap();

        assert_eq!(find_root(&root), Some(hidden.clone()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_dir_merges_file_and_directory_children() {
        let root = temp_dir();
        fs::write(root.join("feature.toon"), "title: Feature").unwrap();
        let child_dir = root.join("feature");
        fs::create_dir_all(&child_dir).unwrap();
        fs::write(child_dir.join("task.toon"), "title: Task").unwrap();

        let tickets = load_dir(&root).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].title, "Feature");
        assert_eq!(tickets[0].children.len(), 1);
        assert_eq!(tickets[0].children[0].title, "Task");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_dir_accepts_json_tickets() {
        let root = temp_dir();
        fs::write(
            root.join("feature.json"),
            r#"{"title":"Feature","status":"todo"}"#,
        )
        .unwrap();

        let tickets = load_dir(&root).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].title, "Feature");
        assert_eq!(tickets[0].raw.status, "todo");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_dir_mixes_toon_and_json_tickets_in_same_dir() {
        let root = temp_dir();
        fs::write(root.join("a.toon"), "title: A").unwrap();
        fs::write(root.join("b.json"), r#"{"title":"B"}"#).unwrap();

        let mut tickets = load_dir(&root).unwrap();
        tickets.sort_by_key(|t| t.title.clone());
        assert_eq!(tickets.len(), 2);
        assert_eq!(tickets[0].title, "A");
        assert_eq!(tickets[1].title, "B");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_dir_prefers_toon_when_both_extensions_share_stem() {
        let root = temp_dir();
        fs::write(root.join("dup.toon"), "title: From TOON").unwrap();
        fs::write(root.join("dup.json"), r#"{"title":"From JSON"}"#).unwrap();

        let tickets = load_dir(&root).unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].title, "From TOON");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_directory_index_picks_up_json_index() {
        let root = temp_dir();
        let dir = root.join("feature");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.json"), r#"{"title":"Indexed"}"#).unwrap();

        let raw = read_directory_index(&dir);
        assert_eq!(raw.title.as_deref(), Some("Indexed"));

        let _ = fs::remove_dir_all(root);
    }
}
