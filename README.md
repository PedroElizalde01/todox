# todox

A fast terminal UI for browsing YAML-based todo tickets.

Tickets live as `.yaml` files in a `.todo/` (or `todo/`) directory. todox renders them as a navigable tree with status, priority, and rich sections — and reloads automatically when files change.

## Install

```bash
cargo install --path .
```

Binary: `todo`

## Usage

Run inside any project containing a `.todo/` or `todo/` directory:

```bash
todo
```

### Keys

| Key | Action |
|-----|--------|
| `j` / `↓` | Down |
| `k` / `↑` | Up |
| `l` / `→` / `Enter` | Open |
| `h` / `←` / `Backspace` | Back |
| `r` | Reload |
| `q` / `Esc` | Quit |

## Ticket format

```yaml
title: Add login flow
status: in-progress
priority: high
estimate: 2d
description: OAuth + session cookie

sections:
  - subtitle: Tasks
    type: checks
    content:
      - { checked: true,  text: design schema }
      - { checked: false, text: write handler }
```

## License

MIT
