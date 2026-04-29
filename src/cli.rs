use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "todo",
    version,
    about = "Browse and convert TOON/JSON todo trees",
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(value_name = "PATH", help = "Project root or todo directory")]
    pub path: Option<PathBuf>,

    #[arg(long, help = "Disable filesystem watch and auto-reload")]
    pub no_watch: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Convert JSON tickets to TOON. Source `.json` files are removed unless `--keep`.
    #[command(name = "json-toon", visible_alias = "j2t")]
    JsonToon(ConvertArgs),

    /// Convert TOON tickets to JSON. Source `.toon` files are removed unless `--keep`.
    #[command(name = "toon-json", visible_alias = "t2j")]
    ToonJson(ConvertArgs),
}

#[derive(Debug, Clone, Args)]
pub struct ConvertArgs {
    #[arg(
        value_name = "PATH",
        help = "Single file or directory. Defaults to nearest .todo/todo dir under cwd."
    )]
    pub path: Option<PathBuf>,

    #[arg(
        short = 'n',
        long,
        help = "Preview changes without writing or deleting"
    )]
    pub dry_run: bool,

    #[arg(short, long, help = "Keep source file after successful conversion")]
    pub keep: bool,

    #[arg(short, long, help = "Overwrite destination if it already exists")]
    pub force: bool,

    #[arg(short, long, help = "Suppress per-file output")]
    pub quiet: bool,
}
