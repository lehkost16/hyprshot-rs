use anyhow::Result;
use clap::Parser;

mod app;
mod capture;
mod capture_session;
mod cli;
mod compositor;
mod config;
mod config_cmds;
mod editor_state;
mod freeze;
mod geometry;
mod workflow;

mod longshot;
mod external;
mod record;
mod save;
mod selector;
mod utils;
pub use cli::{Args, Subcommands, default_filename, resolve_delay, resolve_notif_timeout};

fn main() -> Result<()> {
    let args = Args::parse();
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if args.debug { "debug" } else { "warn" }),
    )
    .try_init();
    app::run(args)
}
#[cfg(test)]
mod tests;
