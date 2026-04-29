use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{cli::ConvertArgs, repository::find_root};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    JsonToToon,
    ToonToJson,
}

impl Direction {
    fn source_ext(self) -> &'static str {
        match self {
            Self::JsonToToon => "json",
            Self::ToonToJson => "toon",
        }
    }

    fn target_ext(self) -> &'static str {
        match self {
            Self::JsonToToon => "toon",
            Self::ToonToJson => "json",
        }
    }
}

#[derive(Debug, Default)]
pub struct Stats {
    pub converted: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub failed: usize,
}

pub fn run(direction: Direction, args: ConvertArgs) -> Result<()> {
    let target = resolve_target(args.path.as_deref())?;
    let files = collect_sources(&target, direction.source_ext())?;

    if files.is_empty() {
        eprintln!(
            "no .{} files found under {}",
            direction.source_ext(),
            target.display()
        );
        return Ok(());
    }

    let mut stats = Stats::default();
    for source in files {
        match convert_one(&source, direction, &args) {
            Ok(Outcome::Converted { deleted }) => {
                stats.converted += 1;
                if deleted {
                    stats.deleted += 1;
                }
            }
            Ok(Outcome::Skipped(reason)) => {
                stats.skipped += 1;
                if !args.quiet {
                    eprintln!("skip {}: {reason}", source.display());
                }
            }
            Err(error) => {
                stats.failed += 1;
                eprintln!("fail {}: {error:#}", source.display());
            }
        }
    }

    print_summary(&stats, direction, args.dry_run);

    if stats.failed > 0 {
        bail!("{} file(s) failed to convert", stats.failed);
    }
    Ok(())
}

#[derive(Debug)]
enum Outcome {
    Converted { deleted: bool },
    Skipped(&'static str),
}

fn convert_one(source: &Path, direction: Direction, args: &ConvertArgs) -> Result<Outcome> {
    let target = source.with_extension(direction.target_ext());

    if target.exists() && !args.force {
        return Ok(Outcome::Skipped("target exists (use --force to overwrite)"));
    }

    let text = fs::read_to_string(source).with_context(|| format!("read {}", source.display()))?;
    let value = decode(&text, direction).with_context(|| format!("decode {}", source.display()))?;
    let output =
        encode(&value, direction).with_context(|| format!("encode {}", target.display()))?;

    if args.dry_run {
        if !args.quiet {
            println!("would write {}", target.display());
        }
        return Ok(Outcome::Converted { deleted: false });
    }

    write_atomic(&target, &output).with_context(|| format!("write {}", target.display()))?;
    if !args.quiet {
        println!("wrote {}", target.display());
    }

    let mut deleted = false;
    if !args.keep {
        fs::remove_file(source).with_context(|| format!("remove {}", source.display()))?;
        deleted = true;
        if !args.quiet {
            println!("removed {}", source.display());
        }
    }

    Ok(Outcome::Converted { deleted })
}

fn decode(text: &str, direction: Direction) -> Result<serde_json::Value> {
    match direction {
        Direction::JsonToToon => serde_json::from_str(text).context("invalid JSON"),
        Direction::ToonToJson => toon_format::decode(text, &toon_format::DecodeOptions::default())
            .map_err(|e| anyhow::anyhow!("invalid TOON: {e}")),
    }
}

fn encode(value: &serde_json::Value, direction: Direction) -> Result<String> {
    match direction {
        Direction::JsonToToon => encode_toon(value),
        Direction::ToonToJson => {
            let mut out = serde_json::to_string_pretty(value).context("json encode")?;
            out.push('\n');
            Ok(out)
        }
    }
}

/// Encode JSON to TOON, then patch a known upstream bug (`toon-format` 0.2.4
/// fails to quote digit-prefix string values like `"5m"`), and validate the
/// output round-trips back to the source value to catch any other corruption.
fn encode_toon(value: &serde_json::Value) -> Result<String> {
    let raw = toon_format::encode_default(value.clone())
        .map_err(|e| anyhow::anyhow!("toon encode: {e}"))?;
    let patched = quote_digit_prefix_scalars(&raw);

    let decoded = toon_format::decode(&patched, &toon_format::DecodeOptions::default())
        .map_err(|e| anyhow::anyhow!("encoded TOON failed to round-trip parse: {e}"))?;
    if decoded != *value {
        bail!(
            "encoded TOON does not round-trip to source value; refusing to write to avoid data loss"
        );
    }
    Ok(patched)
}

/// Patch known under-quoting bugs in `toon-format` 0.2.4 so output round-trips
/// through the strict decoder. Three contexts get fixed:
///
/// 1. `key: value` scalar lines where `value` starts with digit + letter
///    (`5m`, `2d`) → quote the whole value.
/// 2. Tabular rows under `key[N]{cols}:` where any cell contains a space,
///    comma, `--`, or matches the digit-prefix pattern → quote the cell.
/// 3. Inline primitive arrays `key[N]: a,b,c` with the same cell hazards →
///    quote each unsafe cell.
fn quote_digit_prefix_scalars(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out_lines: Vec<String> = Vec::with_capacity(lines.len());
    let mut tabular_remaining: usize = 0;

    for line in &lines {
        if tabular_remaining > 0 {
            out_lines.push(quote_tabular_row(line));
            tabular_remaining -= 1;
            continue;
        }

        if let Some(count) = parse_tabular_header(line) {
            out_lines.push((*line).to_string());
            tabular_remaining = count;
            continue;
        }

        if let Some(fixed) = patch_inline_array(line) {
            out_lines.push(fixed);
            continue;
        }

        out_lines.push(maybe_quote_scalar_line(line));
    }

    let mut out = out_lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// If the line is `key[N]{cols}:` (tabular header), return N. Trailing data
/// after the colon is rare for headers — treated as no header to stay safe.
fn parse_tabular_header(line: &str) -> Option<usize> {
    let rest = line.trim_start();
    let lbrack = rest.find('[')?;
    let key = &rest[..lbrack];
    if key.is_empty() || !key.chars().all(is_key_char) {
        return None;
    }
    let after_lbrack = &rest[lbrack + 1..];
    let rbrack = after_lbrack.find(']')?;
    let count: usize = after_lbrack[..rbrack].parse().ok()?;
    let after_rbrack = after_lbrack[rbrack + 1..].trim_start();
    if !after_rbrack.starts_with('{') {
        return None;
    }
    let after_lbrace = &after_rbrack[1..];
    let rbrace = after_lbrace.find('}')?;
    let after_rbrace = after_lbrace[rbrace + 1..].trim_start();
    if !after_rbrace.starts_with(':') {
        return None;
    }
    // Trailing payload on header (rare) — bail out, let validation catch it.
    if !after_rbrace[1..].trim().is_empty() {
        return None;
    }
    Some(count)
}

fn quote_tabular_row(line: &str) -> String {
    let trimmed_start = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(trimmed_start);
    let cells = split_csv_cells(rest);
    let quoted: Vec<String> = cells.iter().map(|c| quote_cell_if_unsafe(c)).collect();
    format!("{indent}{}", quoted.join(","))
}

/// Match `key[N]: a,b,c` (inline primitive array) and quote unsafe cells.
/// Returns `None` for non-matching lines so the caller can fall through.
fn patch_inline_array(line: &str) -> Option<String> {
    let trimmed_start = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(trimmed_start);
    let lbrack = rest.find('[')?;
    let key = &rest[..lbrack];
    if key.is_empty() || !key.chars().all(is_key_char) {
        return None;
    }
    let after_lbrack = &rest[lbrack + 1..];
    let rbrack = after_lbrack.find(']')?;
    after_lbrack[..rbrack].parse::<usize>().ok()?;
    let after_rbrack = after_lbrack[rbrack + 1..].trim_start();
    // Tabular headers handled separately.
    if after_rbrack.starts_with('{') {
        return None;
    }
    if !after_rbrack.starts_with(':') {
        return None;
    }
    let payload = after_rbrack[1..].trim_start();
    if payload.is_empty() {
        return None;
    }
    let cells = split_csv_cells(payload);
    let quoted: Vec<String> = cells.iter().map(|c| quote_cell_if_unsafe(c)).collect();
    let header = &rest[..rest.len() - after_rbrack.len()];
    Some(format!("{indent}{header}: {}", quoted.join(",")))
}

/// Split a CSV-like row, respecting `"..."` quoted cells. Backslash escapes
/// inside quotes are preserved verbatim (matches TOON's tolerant tokenizer).
fn split_csv_cells(s: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quote = !in_quote;
                current.push(c);
            }
            '\\' if in_quote => {
                current.push(c);
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ',' if !in_quote => {
                cells.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    cells.push(current);
    cells
}

fn quote_cell_if_unsafe(cell: &str) -> String {
    let trimmed = cell.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        return cell.to_string();
    }
    if trimmed.is_empty() || !cell_needs_quoting(trimmed) {
        return cell.to_string();
    }
    let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// True if `s` cannot be parsed as a bare cell in tabular/inline-array context.
fn cell_needs_quoting(s: &str) -> bool {
    if s.contains(' ') || s.contains('\t') || s.contains(',') || s.contains("--") || s.contains(':')
    {
        return true;
    }
    needs_string_quoting(s)
}

fn maybe_quote_scalar_line(line: &str) -> String {
    let trimmed_start = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(trimmed_start);

    // Skip list items and comments — only `key: value` lines are in scope.
    if rest.starts_with('-') || rest.starts_with('#') {
        return line.to_string();
    }

    let Some(colon) = rest.find(':') else {
        return line.to_string();
    };
    let key = &rest[..colon];
    if key.is_empty() || !key.chars().all(is_key_char) {
        return line.to_string();
    }
    // `key[...]` or `key{...}` mean an array/tabular header — value is structural.
    if key.contains('[') || key.contains('{') {
        return line.to_string();
    }

    let after = &rest[colon + 1..];
    let value = after.trim_start();
    if value.is_empty() || value.starts_with('"') {
        return line.to_string();
    }
    if !needs_string_quoting(value) {
        return line.to_string();
    }

    let space = if after.starts_with(' ') { " " } else { "" };
    format!("{indent}{key}:{space}\"{}\"", value.trim_end())
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// True if `s` would be parsed as a number-with-junk (e.g. `5m`) by the TOON
/// decoder but is intended as a string literal.
fn needs_string_quoting(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_digit() {
        return false;
    }
    let mut saw_letter = false;
    let mut seen_decimal = false;
    for c in chars {
        if c.is_ascii_digit() {
            continue;
        }
        if c == '.' && !seen_decimal {
            seen_decimal = true;
            continue;
        }
        if c.is_ascii_alphabetic() {
            saw_letter = true;
            continue;
        }
        // Anything else (punctuation, spaces) — let the encoder handle it.
        return false;
    }
    saw_letter
}

/// Write to a temp sibling then rename, so a crash mid-write cannot corrupt the file.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = parent.join(format!(".{file_name}.tmp"));
    fs::write(&tmp, contents).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn collect_sources(target: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if target.is_file() {
        if target.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(target.to_path_buf());
        }
        return Ok(out);
    }
    walk(target, ext, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, ext, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
    Ok(())
}

fn resolve_target(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.exists() {
            bail!("path does not exist: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("read current dir")?;
    if let Some(name) = cwd.file_name().and_then(|n| n.to_str()) {
        if matches!(name, ".todo" | "todo") {
            return Ok(cwd);
        }
    }
    find_root(&cwd).context("no .todo or todo directory found in current dir")
}

fn print_summary(stats: &Stats, direction: Direction, dry_run: bool) {
    let label = match direction {
        Direction::JsonToToon => "json -> toon",
        Direction::ToonToJson => "toon -> json",
    };
    let prefix = if dry_run { "[dry-run] " } else { "" };
    eprintln!(
        "{prefix}{label}: {} converted, {} skipped, {} deleted, {} failed",
        stats.converted, stats.skipped, stats.deleted, stats.failed
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("todox-conv-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(path: PathBuf) -> ConvertArgs {
        ConvertArgs {
            path: Some(path),
            dry_run: false,
            keep: false,
            force: false,
            quiet: true,
        }
    }

    #[test]
    fn json_to_toon_round_trip_preserves_schema() {
        let dir = temp_dir();
        let json = dir.join("a.json");
        fs::write(&json, r#"{"title":"X","status":"todo","sections":[]}"#).unwrap();

        run(Direction::JsonToToon, args(dir.clone())).unwrap();

        assert!(!json.exists(), "source removed by default");
        let toon_path = dir.join("a.toon");
        assert!(toon_path.exists());
        let text = fs::read_to_string(&toon_path).unwrap();
        let decoded = toon_format::decode(&text, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(decoded["title"], "X");
        assert_eq!(decoded["status"], "todo");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keep_flag_preserves_source() {
        let dir = temp_dir();
        let json = dir.join("a.json");
        fs::write(&json, r#"{"title":"X"}"#).unwrap();

        let mut a = args(dir.clone());
        a.keep = true;
        run(Direction::JsonToToon, a).unwrap();

        assert!(json.exists(), "source kept with --keep");
        assert!(dir.join("a.toon").exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = temp_dir();
        let json = dir.join("a.json");
        fs::write(&json, r#"{"title":"X"}"#).unwrap();

        let mut a = args(dir.clone());
        a.dry_run = true;
        run(Direction::JsonToToon, a).unwrap();

        assert!(json.exists(), "source untouched on dry-run");
        assert!(!dir.join("a.toon").exists(), "no target written on dry-run");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn skip_when_target_exists_without_force() {
        let dir = temp_dir();
        fs::write(dir.join("a.json"), r#"{"title":"FromJson"}"#).unwrap();
        fs::write(dir.join("a.toon"), "title: Existing").unwrap();

        run(Direction::JsonToToon, args(dir.clone())).unwrap();

        assert!(dir.join("a.json").exists(), "source untouched on skip");
        let text = fs::read_to_string(dir.join("a.toon")).unwrap();
        assert!(text.contains("Existing"), "target preserved on skip");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn force_overwrites_target() {
        let dir = temp_dir();
        fs::write(dir.join("a.json"), r#"{"title":"Fresh"}"#).unwrap();
        fs::write(dir.join("a.toon"), "title: Old").unwrap();

        let mut a = args(dir.clone());
        a.force = true;
        run(Direction::JsonToToon, a).unwrap();

        let text = fs::read_to_string(dir.join("a.toon")).unwrap();
        assert!(text.contains("Fresh"));
        assert!(!dir.join("a.json").exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn toon_to_json_round_trip() {
        let dir = temp_dir();
        let toon = dir.join("a.toon");
        fs::write(&toon, "title: Y\nstatus: done\n").unwrap();

        run(Direction::ToonToJson, args(dir.clone())).unwrap();

        assert!(!toon.exists());
        let json = dir.join("a.json");
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(parsed["title"], "Y");
        assert_eq!(parsed["status"], "done");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn walks_subdirectories() {
        let dir = temp_dir();
        fs::write(dir.join("a.json"), r#"{"title":"A"}"#).unwrap();
        let sub = dir.join("nested");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.json"), r#"{"title":"B"}"#).unwrap();

        run(Direction::JsonToToon, args(dir.clone())).unwrap();

        assert!(dir.join("a.toon").exists());
        assert!(sub.join("b.toon").exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quote_digit_prefix_patches_estimate_value() {
        let raw = "title: t\nestimate: 5m\nstatus: todo\n";
        let patched = quote_digit_prefix_scalars(raw);
        assert!(patched.contains("estimate: \"5m\""), "patched: {patched}");
        assert!(patched.contains("title: t"));
    }

    #[test]
    fn quote_digit_prefix_leaves_pure_numbers_alone() {
        let raw = "count: 42\nratio: 1.5\n";
        let patched = quote_digit_prefix_scalars(raw);
        assert_eq!(patched, raw);
    }

    #[test]
    fn quote_digit_prefix_skips_array_headers() {
        let raw = "items[3]:\n  - one\n  - two\n  - three\n";
        let patched = quote_digit_prefix_scalars(raw);
        assert_eq!(patched, raw);
    }

    #[test]
    fn quote_unsafe_tabular_cells() {
        let raw = "rows[2]{a,b}:\n  1,Long titles may wrap badly\n  2,short\n";
        let patched = quote_digit_prefix_scalars(raw);
        assert!(
            patched.contains("\"Long titles may wrap badly\""),
            "patched: {patched}"
        );
        assert!(patched.contains("2,short"));
        let decoded =
            toon_format::decode(&patched, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(decoded["rows"][0]["b"], "Long titles may wrap badly");
    }

    #[test]
    fn quote_unsafe_inline_array_cells() {
        let raw = "items[2]: Long titles may wrap badly,short\n";
        let patched = quote_digit_prefix_scalars(raw);
        assert!(patched.contains("\"Long titles may wrap badly\""));
        let decoded =
            toon_format::decode(&patched, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(decoded["items"][0], "Long titles may wrap badly");
        assert_eq!(decoded["items"][1], "short");
    }

    #[test]
    fn round_trip_with_multiword_array_strings() {
        let dir = temp_dir();
        let json = dir.join("a.json");
        fs::write(
            &json,
            r#"{"title":"X","sections":[{"subtitle":"Risks","content":["Long titles may wrap badly","Deep nesting may need scroll support"]}]}"#,
        )
        .unwrap();

        run(Direction::JsonToToon, args(dir.clone())).unwrap();

        let toon = fs::read_to_string(dir.join("a.toon")).unwrap();
        let decoded = toon_format::decode(&toon, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(
            decoded["sections"][0]["content"][0],
            "Long titles may wrap badly"
        );
        assert_eq!(
            decoded["sections"][0]["content"][1],
            "Deep nesting may need scroll support"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn round_trip_with_digit_prefix_string() {
        let dir = temp_dir();
        let json = dir.join("a.json");
        fs::write(&json, r#"{"title":"X","estimate":"5m","status":"todo"}"#).unwrap();

        run(Direction::JsonToToon, args(dir.clone())).unwrap();

        let toon = fs::read_to_string(dir.join("a.toon")).unwrap();
        let decoded = toon_format::decode(&toon, &toon_format::DecodeOptions::default()).unwrap();
        assert_eq!(decoded["estimate"], "5m");
        assert_eq!(decoded["title"], "X");
        assert_eq!(decoded["status"], "todo");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn single_file_target_works() {
        let dir = temp_dir();
        let json = dir.join("only.json");
        fs::write(&json, r#"{"title":"Solo"}"#).unwrap();
        // Sibling JSON should not be touched when target is a single file.
        fs::write(dir.join("other.json"), r#"{"title":"Other"}"#).unwrap();

        run(Direction::JsonToToon, args(json.clone())).unwrap();

        assert!(dir.join("only.toon").exists());
        assert!(!json.exists());
        assert!(dir.join("other.json").exists(), "sibling untouched");
        assert!(!dir.join("other.toon").exists());

        let _ = fs::remove_dir_all(dir);
    }
}
