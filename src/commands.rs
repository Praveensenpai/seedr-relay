//! CLI command implementations for seedr-relay.

use crate::config::{load_config, save_config};
use crate::db::{QueueDb, QueueStatus};
use crate::worker;
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Handles the `add` command.
///
/// # Errors
/// Returns an error if queue insertion fails.
pub fn handle_add(magnets: &[String], output: Option<&Path>) -> Result<()> {
    let db = QueueDb::open()?;

    for magnet in magnets {
        let item = db.enqueue(magnet, output)?;
        println!(
            "  {} Enqueued [{}] {}",
            "✔".green().bold(),
            item.id,
            item.name.cyan()
        );
        if let Some(ref out) = item.output_dir {
            println!("    Destination: {}", out.display());
        }
    }
    Ok(())
}

/// Handles the `run` command.
///
/// # Errors
/// Returns an error if worker processing fails.
pub async fn handle_run(watch: bool, output: Option<PathBuf>) -> Result<()> {
    let mut config = load_config()?;
    if let Some(out) = output {
        config.save_path = out;
    }
    let db = QueueDb::open()?;
    worker::process_queue(&config, &db, watch).await
}

/// Handles the `list` command.
///
/// # Errors
/// Returns an error if querying the queue fails.
pub fn handle_list(limit: usize) -> Result<()> {
    let db = QueueDb::open()?;
    let items = db.list_items(limit)?;

    if items.is_empty() {
        println!("  {}", "Queue is empty.".yellow());
        return Ok(());
    }

    println!("  {}", "Seedr Relay Queue:".cyan().bold());
    println!("  {:<4} {:<12} {:<40} {:<20}", "ID", "STATUS", "NAME", "CREATED");
    println!("  {}", "-".repeat(78));

    for item in items {
        let status_str = item.status.as_str();
        let status_colored = match item.status {
            QueueStatus::Pending => status_str.yellow(),
            QueueStatus::Caching => status_str.blue(),
            QueueStatus::Downloading => status_str.cyan(),
            QueueStatus::Completed => status_str.green(),
            QueueStatus::Failed => status_str.red(),
        };

        let pad_len = 12_usize.saturating_sub(status_str.len());
        let status_display = format!("{status_colored}{}", " ".repeat(pad_len));

        let short_name = if item.name.len() > 38 {
            format!("{}...", &item.name[..35])
        } else {
            item.name.clone()
        };

        let short_date = if item.created_at.len() > 19 {
            &item.created_at[..19]
        } else {
            &item.created_at
        };

        println!(
            "  {:<4} {} {:<40} {:<20}",
            item.id, status_display, short_name, short_date
        );

        if let Some(ref err) = item.error_msg {
            println!("       {} {}", "Error:".red(), err);
        }
    }
    Ok(())
}

/// Handles the `remove` command.
///
/// # Errors
/// Returns an error if deletion from database fails.
pub fn handle_remove(id: i64) -> Result<()> {
    let db = QueueDb::open()?;
    if db.remove_item(id)? {
        println!("  {} Removed item [{id}] from queue.", "✔".green());
    } else {
        println!("  {} No queue item found with ID [{id}].", "✖".red());
    }
    Ok(())
}

/// Handles the `clean` command.
///
/// # Errors
/// Returns an error if database clean fails.
pub fn handle_clean() -> Result<()> {
    let db = QueueDb::open()?;
    let removed = db.clean_completed()?;
    println!("  {} Cleared {removed} completed items from queue.", "✔".green());
    Ok(())
}

/// Handles the `config` command.
///
/// # Errors
/// Returns an error if reading or saving config fails.
pub fn handle_config(
    save_path: Option<PathBuf>,
    notify: Option<bool>,
    show: bool,
) -> Result<()> {
    let mut cfg = load_config()?;

    let updated = save_path.is_some() || notify.is_some();

    if let Some(path) = save_path {
        cfg.save_path = path;
        save_config(&cfg).context("failed to save updated config")?;
        println!("  {} Updated default save path.", "✔".green());
    }

    if let Some(notif) = notify {
        cfg.notify = notif;
        save_config(&cfg).context("failed to save updated config")?;
        println!("  {} Updated notification setting.", "✔".green());
    }

    if show || !updated {
        println!("  {}", "Current Configuration:".cyan().bold());
        println!("  • Save Path         : {}", cfg.save_path.display());
        println!("  • Tayori Notify     : {}", cfg.notify);
        println!("  • Auto Clean Cloud  : {}", cfg.auto_clean_cloud);
        println!("  • Poll Interval     : {}s", cfg.poll_interval_secs);
    }
    Ok(())
}
