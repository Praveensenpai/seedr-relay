//! CLI command implementations for seedr-relay.

use crate::config::{load_config, save_config};
use crate::daemon;
use crate::db::{QueueDb, QueueItem, QueueStatus};
use crate::worker;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
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
/// Returns an error if worker processing or daemon spawn fails.
pub async fn handle_run(watch: bool, daemon: bool, output: Option<PathBuf>) -> Result<()> {
    if daemon {
        return daemon::spawn_daemon(output.as_deref());
    }

    let mut config = load_config()?;
    if let Some(out) = output {
        config.save_path = out;
    }
    let db = QueueDb::open()?;
    worker::process_queue(&config, &db, watch).await
}

/// Handles the `attach` command.
///
/// # Errors
/// Returns an error if log streaming fails.
pub async fn handle_attach() -> Result<()> {
    daemon::attach_daemon().await
}

/// Handles the `stop` command.
///
/// # Errors
/// Returns an error if stopping the daemon fails.
pub async fn handle_stop() -> Result<()> {
    daemon::stop_daemon().await
}

/// Handles the `retry` command.
///
/// # Errors
/// Returns an error if database update fails.
pub fn handle_retry(id: Option<i64>) -> Result<()> {
    let db = QueueDb::open()?;
    let count = db.retry(id)?;
    if let Some(target_id) = id {
        if count > 0 {
            println!("  {} Reset item [{target_id}] back to pending.", "✔".green());
        } else {
            println!("  {} No item found with ID [{target_id}].", "✖".red());
        }
    } else {
        println!("  {} Reset {count} failed item(s) back to pending.", "✔".green());
    }
    Ok(())
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
    println!(
        "  {:<4} {:<14} {:<32} {:<40} {:<12}",
        "ID", "STATUS", "PROGRESS", "NAME", "ADDED"
    );
    println!("  {}", "-".repeat(106));

    let active = daemon::load_active_progress();
    for item in items {
        print_queue_row(&item, active.as_ref());
    }
    Ok(())
}

fn print_queue_row(item: &QueueItem, active: Option<&daemon::ActiveProgress>) {
    let status_str = item.status.as_str();
    let status_colored = match item.status {
        QueueStatus::Pending => status_str.yellow(),
        QueueStatus::Caching => status_str.blue(),
        QueueStatus::Downloading => status_str.cyan(),
        QueueStatus::Completed => status_str.green(),
        QueueStatus::Failed => status_str.red(),
    };

    let status_pad = " ".repeat(14_usize.saturating_sub(status_str.len()));
    let status_display = format!("{status_colored}{status_pad}");

    let progress_str = format_progress_col(item, active);
    let progress_pad = " ".repeat(32_usize.saturating_sub(visible_len(&progress_str)));

    let short_name = if item.name.len() > 38 {
        format!("{}...", &item.name[..35])
    } else {
        item.name.clone()
    };

    let human_time = format_human_time(&item.created_at);

    println!(
        "  {:<4} {} {progress_str}{progress_pad} {:<40} {:<12}",
        item.id, status_display, short_name, human_time
    );

    if let Some(ref err) = item.error_msg {
        println!("       {} {}", "Error:".red(), err);
    }
}

fn format_progress_col(item: &QueueItem, active: Option<&daemon::ActiveProgress>) -> String {
    match item.status {
        QueueStatus::Pending => "queued".dimmed().to_string(),
        QueueStatus::Caching => "caching in cloud".blue().to_string(),
        QueueStatus::Downloading => format_downloading_progress(active, item.id),
        QueueStatus::Completed => "100% complete".green().to_string(),
        QueueStatus::Failed => "failed".red().to_string(),
    }
}

fn format_downloading_progress(active: Option<&daemon::ActiveProgress>, item_id: i64) -> String {
    let Some(a) = active.filter(|a| a.item_id == item_id || a.item_id == 0) else {
        return "downloading...".cyan().to_string();
    };

    #[allow(clippy::cast_precision_loss)]
    let total_gb = a.total_bytes as f64 / 1_073_741_824.0;
    #[allow(clippy::cast_precision_loss)]
    let dl_gb = a.downloaded_bytes as f64 / 1_073_741_824.0;

    if a.speed_bps > 0 {
        #[allow(clippy::cast_precision_loss)]
        let mb_s = a.speed_bps as f64 / 1_048_576.0;
        let eta_m = a.eta_seconds / 60;
        let eta_s = a.eta_seconds % 60;
        format!(
            "{:5.1}% ({mb_s:>4.1} MB/s, ETA {eta_m}m{eta_s:02}s)",
            a.percentage
        )
        .cyan()
        .to_string()
    } else {
        format!("{:5.1}% ({dl_gb:.2}/{total_gb:.2} GiB)", a.percentage)
            .cyan()
            .to_string()
    }
}

/// Converts an ISO-8601 UTC timestamp to a human-readable relative duration.
#[must_use]
pub fn format_human_time(iso_str: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso_str) else {
        return iso_str.get(..19).unwrap_or(iso_str).to_string();
    };
    let now = Utc::now();
    let diff = now.signed_duration_since(dt.with_timezone(&Utc));
    let secs = diff.num_seconds();

    if secs < 0 || secs < 45 {
        "just now".to_string()
    } else if secs < 90 {
        "1m ago".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 7200 {
        "1h ago".to_string()
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 172_800 {
        "yesterday".to_string()
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn visible_len(s: &str) -> usize {
    let mut in_esc = false;
    let mut len = 0;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
        } else if in_esc {
            if c == 'm' {
                in_esc = false;
            }
        } else {
            len += 1;
        }
    }
    len
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
    let staging_base = crate::db::data_dir().join("staging");
    if staging_base.exists() {
        let _ = std::fs::remove_dir_all(staging_base);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_human_time_returns_relative_strings() {
        let now_str = Utc::now().to_rfc3339();
        assert_eq!(format_human_time(&now_str), "just now");

        let five_mins_ago = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_human_time(&five_mins_ago), "5m ago");

        let two_hours_ago = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert_eq!(format_human_time(&two_hours_ago), "2h ago");
    }
}
