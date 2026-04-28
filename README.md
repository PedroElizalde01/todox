# todox

A fast terminal UI for browsing TOON-based todo tickets.

Tickets live as `.toon` files in a `.todo/` (or `todo/`) directory. todox renders them as a navigable tree with status, priority, and rich sections — and reloads automatically when files change.

[TOON](https://toonformat.dev/) (Token-Oriented Object Notation) is a compact, LLM-friendly format that uses ~40-60% fewer tokens than JSON while staying human-readable. Ideal for agent-written tickets.

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

```
title: Add login flow
status: in-progress
priority: high
estimate: 2d
description: OAuth + session cookie
sections[1]:
  subtitle: Tasks
  type: checks
  content[2]{checked,text}:
    true,design schema
    false,write handler
```

## License

MIT
