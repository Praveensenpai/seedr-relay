//! Seedr Relay - Sequential queue manager and file sanitizer using seedr-dl.

mod cleaner;
mod commands;
mod config;
mod daemon;
mod db;
mod fmt;
mod notify;
mod worker;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "seedr-relay")]
#[command(author = "Praveensenpai <pvnt20@gmail.com>")]
#[command(version)]
#[command(about = "Sequential queue manager and file sanitizer for Seedr.cc")]
struct Cli {
    /// Direct magnet link to add to queue
    #[arg(value_name = "MAGNET")]
    magnet: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add one or more magnet links to the queue
    Add {
        /// Magnet links or torrent URLs
        #[arg(required = true)]
        magnets: Vec<String>,

        /// Custom destination directory for this download
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },

    /// Run the queue worker to process downloads sequentially
    Run {
        /// Continuously watch queue for new incoming magnets
        #[arg(short = 'w', long = "watch")]
        watch: bool,

        /// Run in background as an asynchronous daemon
        #[arg(short = 'd', long = "daemon")]
        daemon: bool,

        /// Override global save directory for this run
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },

    /// Attach to live background daemon output logs
    Attach,

    /// Gracefully stop the running background daemon
    Stop,

    /// Reset failed item(s) back to pending so they are re-attempted
    Retry {
        /// Specific queue item ID to retry (or omit to retry all failed)
        id: Option<i64>,
    },

    /// Promote a pending item to be downloaded next
    #[command(alias = "bump", alias = "priority", alias = "top")]
    Next {
        /// Queue item ID to prioritize
        id: i64,
    },

    /// List queued, active, and completed downloads
    List {
        /// Maximum number of items to display
        #[arg(short = 'n', long = "limit", default_value = "50")]
        limit: usize,
    },

    /// Remove an item from the queue by ID
    Remove {
        /// Queue item ID
        id: i64,
    },

    /// Clear all completed items from the queue
    Clean,

    /// View or update configuration (save path, notifications)
    Config {
        /// Update default local save path
        #[arg(long = "save-path")]
        save_path: Option<PathBuf>,

        /// Enable or disable Tayori notifications (true/false)
        #[arg(long = "notify")]
        notify: Option<bool>,

        /// Display current configuration
        #[arg(long = "show")]
        show: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Add { magnets, output }) => {
            commands::handle_add(&magnets, output.as_deref())?;
        }
        Some(Commands::Run { watch, daemon, output }) => {
            commands::handle_run(watch, daemon, output).await?;
        }
        Some(Commands::Attach) => {
            commands::handle_attach().await?;
        }
        Some(Commands::Stop) => {
            commands::handle_stop().await?;
        }
        Some(Commands::Retry { id }) => {
            commands::handle_retry(id)?;
        }
        Some(Commands::Next { id }) => {
            commands::handle_next(id)?;
        }
        Some(Commands::List { limit }) => {
            commands::handle_list(limit)?;
        }
        Some(Commands::Remove { id }) => {
            commands::handle_remove(id)?;
        }
        Some(Commands::Clean) => {
            commands::handle_clean()?;
        }
        Some(Commands::Config { save_path, notify, show }) => {
            commands::handle_config(save_path, notify, show)?;
        }
        None => {
            if let Some(magnet) = cli.magnet {
                commands::handle_add(&[magnet], None)?;
            } else {
                commands::handle_list(20)?;
            }
        }
    }
    Ok(())
}
