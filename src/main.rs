use anyhow::Result;
use clap::Parser;

mod app;
mod cli;
mod config;
mod git;
mod lock;
mod repo;

fn main() -> Result<()> {
    app::run(cli::Cli::parse())
}
