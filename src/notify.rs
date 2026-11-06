//! Notifications using the `tayori` CLI.

use anyhow::{bail, Context, Result};
use tokio::process::Command;

/// Sends a completion card via `tayori done`.
pub async fn done(title: &str, body: &str) {
    let message = format!("{title}\n{body}");
    if let Err(err) = execute(&["done", &message]).await {
        eprintln!("tayori notification skipped: {err:#}");
    }
}

/// Sends an alert card via `tayori alert`.
pub async fn alert(title: &str, body: &str) {
    let message = format!("{title}\n{body}");
    if let Err(err) = execute(&["alert", &message]).await {
        eprintln!("tayori alert skipped: {err:#}");
    }
}

async fn execute(args: &[&str]) -> Result<()> {
    let status = Command::new("tayori")
        .args(args)
        .status()
        .await
        .context("failed to invoke tayori CLI")?;

    if !status.success() {
        bail!("tayori command exited with status {status}");
    }
    Ok(())
}
