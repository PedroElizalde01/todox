use clap::Parser;

fn main() -> anyhow::Result<()> {
    todox::run(todox::cli::Cli::parse())
}
