use anyhow::Result;
use clap::Parser;

mod app;
mod capture;
mod cli;
mod config;
mod config_cmds;
mod freeze;
mod geometry;
mod hyprland_cmds;
mod save;
mod selector;
mod utils;
mod external;
mod longshot;
pub use cli::{Args, Subcommands, default_filename, resolve_delay, resolve_notif_timeout};

fn main() -> Result<()> {
    let args = Args::parse();
    app::run(args)
}
#[cfg(test)]
mod tests;
