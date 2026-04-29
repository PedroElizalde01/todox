<div align="center">
<pre>
 ███████████    ███████    ██████████      ███████    █████ █████
░█░░░███░░░█  ███░░░░░███ ░░███░░░░███   ███░░░░░███ ░░███ ░░███ 
░   ░███  ░  ███     ░░███ ░███   ░░███ ███     ░░███ ░░███ ███  
    ░███    ░███      ░███ ░███    ░███░███      ░███  ░░█████   
    ░███    ░███      ░███ ░███    ░███░███      ░███   ███░███  
    ░███    ░░███     ███  ░███    ███ ░░███     ███   ███ ░░███ 
    █████    ░░░███████░   ██████████   ░░░███████░   █████ █████
   ░░░░░       ░░░░░░░    ░░░░░░░░░░      ░░░░░░░    ░░░░░ ░░░░░ 
                                                                 
                                                                 
                                                                 
</pre>

TUI for browsing TOON- or JSON- based todo tickets
</div>

---

## Project layout

- `src/main.rs` — binary bootstrap
- `src/lib.rs` — runtime orchestration
- `.github/workflows/ci.yml` — CI checks

## Install

`cargo install --path .`

Binary: `todo`

## Commands

### TUI (default)

| Command | Effect |
|---|---|
| `todo` | Browse `.todo/` or `todo/` under cwd |
| `todo PATH` | Browse the given dir or project root |
| `todo --no-watch` | Disable filesystem auto-reload |
| `todo -h` / `--help` | Help |
| `todo -V` / `--version` | Version |

### Convert

| Command | Effect |
|---|---|
| `todo json-toon` (alias `j2t`) | Convert `.json` → `.toon` in nearest todo dir |
| `todo json-toon PATH` | Convert single file or recurse a directory |
| `todo toon-json` (alias `t2j`) | Reverse direction |
| `todo toon-json PATH` | Same on the given path |

Convert flags (apply to both directions):

| Flag | Effect |
|---|---|
| `-n`, `--dry-run` | Preview without writing or deleting |
| `-k`, `--keep` | Keep source file after successful conversion |
| `-f`, `--force` | Overwrite destination if it already exists |
| `-q`, `--quiet` | Suppress per-file output |
| `-h`, `--help` | Subcommand help |

Conversions write atomically (temp file + rename) and the JSON→TOON path validates that the encoded output round-trips back to the source value before writing — so an encoder bug cannot silently destroy data.

### TUI keybindings (in-app)

| Key | Action |
|---|---|
| `j` / `↓` | Down |
| `k` / `↑` | Up |
| `l` / `→` / `Enter` | Open / drill in |
| `h` / `←` / `Backspace` | Back |
| `r` | Reload |
| `q` / `Esc` | Back, then quit at root |
| `Q` | Force quit |

### Dev

| Command | Effect |
|---|---|
| `cargo build` | Build debug |
| `cargo run -- ...` | Run with args |
| `cargo test` | Run tests |
| `cargo fmt` | Format |
| `cargo clippy --all-targets --all-features` | Lint |
| `cargo install --path .` | Install `todo` binary |

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
