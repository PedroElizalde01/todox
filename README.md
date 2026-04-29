# todox

Terminal UI for browsing TOON- or JSON-based todo tickets.

## Why this repo better now

- Thin binary entrypoint: `src/main.rs`
- Reusable library core: `src/lib.rs`
- Clear module split:
  - `src/app.rs` — app state + navigation
  - `src/repository.rs` — ticket loading (TOON + JSON) + filesystem traversal
  - `src/ui.rs` — rendering
  - `src/tui.rs` — terminal lifecycle
  - `src/model.rs` — domain types
- Unit tests for repo-loading behavior
- CI for fmt, clippy, tests

## Project layout

- `src/main.rs` — binary bootstrap
- `src/lib.rs` — runtime orchestration
- `.github/workflows/ci.yml` — CI checks

## Install

`cargo install --path .`

Binary: `todo`

## Usage

Run inside project containing `.todo/` or `todo/` directory:

`todo`

Use custom root:

`todo /path/to/project`

Disable auto-reload watch:

`todo --no-watch`

## Keys

| Key | Action |
|-----|--------|
| `j` / `↓` | Down |
| `k` / `↑` | Up |
| `l` / `→` / `Enter` | Open |
| `h` / `←` / `Backspace` | Back |
| `r` | Reload |
| `q` / `Esc` | Back / Quit |
| `Q` | Force quit |

## Dev

- `cargo fmt`
- `cargo clippy --all-targets --all-features`
- `cargo test`
- `cargo run -- --help`

## Ticket files

Tickets live in `.todo/` (or `todo/`) as either `.toon` or `.json` files. Both formats share the same schema; mix freely. If two files share a stem (`foo.toon` + `foo.json`), TOON wins and a conflict warning is printed on load. Subtickets are nested by colocating a directory next to a ticket file with the same stem (`foo.toon` + `foo/*.toon|json`).

## Ticket format

```
title: Add login flow
status: in-progress
priority: high
estimate: "2d"
description: "OAuth + session cookie"
sections[1]:
  - subtitle: Tasks
    type: checks
    content[2]{checked,text}:
      true,"design schema"
      false,"write handler"
```

Notes for hand-written tickets:

- Quote any string containing `,` `--` or that starts with a digit (`"1d"`, `"2h"`).
- `sections[N]:` is a list of objects — each section starts with `- ` and the rest of its keys indent under the dash.
- Primitive `content[N]:` arrays use `- item` lines, one per entry.
- Tabular `content[N]{cols}:` rows are comma-separated; quote any cell with commas or `--`.

The same ticket as JSON:

```json
{
  "title": "Add login flow",
  "status": "in-progress",
  "priority": "high",
  "estimate": "2d",
  "description": "OAuth + session cookie",
  "sections": [
    {
      "subtitle": "Tasks",
      "type": "checks",
      "content": [
        { "checked": true, "text": "design schema" },
        { "checked": false, "text": "write handler" }
      ]
    }
  ]
}
```

## License

MIT
