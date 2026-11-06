//! Background daemon lifecycle management and log attachment.

use crate::db::data_dir;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

/// Returns the path to the daemon PID file: `~/.local/share/seedr-relay/daemon.pid`.
#[must_use]
pub fn pid_path() -> PathBuf {
    data_dir().join("daemon.pid")
}

/// Returns the path to the daemon log file: `~/.local/share/seedr-relay/daemon.log`.
#[must_use]
pub fn log_path() -> PathBuf {
    data_dir().join("daemon.log")
}

/// Spawns the worker as a background daemon process.
///
/// # Errors
/// Returns an error if the daemon is already running or cannot be spawned.
pub fn spawn_daemon(output: Option<&std::path::Path>) -> Result<()> {
    if let Some(pid) = read_running_pid()? {
        anyhow::bail!("Seedr Relay daemon is already running (PID: {pid})");
    }

    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let dir = data_dir();
    fs::create_dir_all(&dir)?;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .context("failed to open daemon.log")?;

    let stderr_file = log_file.try_clone().context("failed to clone log file handle")?;

    let mut args = vec!["run".to_string(), "--watch".to_string()];
    if let Some(out) = output {
        args.push("--output".to_string());
        args.push(out.to_string_lossy().to_string());
    }

    let child = Command::new(exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .context("failed to spawn daemon process")?;

    let pid = child.id();
    fs::write(pid_path(), pid.to_string())
        .context("failed to write daemon PID file")?;

    println!("  {} Seedr Relay daemon started (PID: {})", "✔".green().bold(), pid);
    println!("  • View live logs : {}", "seedr-relay attach".cyan());
    println!("  • Stop daemon    : {}", "seedr-relay stop".cyan());
    Ok(())
}

/// Attaches to live daemon output, streaming newly appended lines until Ctrl+C.
///
/// # Errors
/// Returns an error if reading the log file fails.
pub async fn attach_daemon() -> Result<()> {
    let path = log_path();
    if !path.exists() {
        println!("  {}", "No daemon log found. Is the daemon running?".yellow());
        return Ok(());
    }

    let pid_info = match read_running_pid()? {
        Some(pid) => format!("(PID: {pid})"),
        None => "(daemon not running)".dimmed().to_string(),
    };

    println!(
        "  {} Attaching to daemon output {}... (Press Ctrl+C to detach)",
        "==>".cyan().bold(),
        pid_info
    );

    let mut file = File::open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    // Seek near end: last 4KB or beginning
    let meta = file.metadata()?;
    let start_pos = meta.len().saturating_sub(4096);
    file.seek(SeekFrom::Start(start_pos))?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                sleep(Duration::from_millis(300)).await;
            }
            Ok(_) => {
                print!("{line}");
            }
            Err(e) => {
                eprintln!("Error reading log: {e}");
                break;
            }
        }
    }
    Ok(())
}

/// Stops the running daemon process gracefully.
///
/// # Errors
/// Returns an error if terminating the process fails.
pub async fn stop_daemon() -> Result<()> {
    let Some(pid) = read_running_pid()? else {
        println!("  {}", "Seedr Relay daemon is not running.".yellow());
        clean_pid_file();
        return Ok(());
    };

    println!("  Terminating daemon (PID: {pid})...");

    #[cfg(unix)]
    {
        let pid_i32 = i32::try_from(pid).unwrap_or(0);
        let _ = Command::new("kill").args(["-TERM", &pid_i32.to_string()]).status();

        for _ in 0..20 {
            sleep(Duration::from_millis(200)).await;
            if !is_process_alive(pid) {
                break;
            }
        }

        if is_process_alive(pid) {
            let _ = Command::new("kill").args(["-KILL", &pid_i32.to_string()]).status();
        }
    }

    clean_pid_file();
    println!("  {} Seedr Relay daemon stopped.", "✔".green());
    Ok(())
}

fn clean_pid_file() {
    let _ = fs::remove_file(pid_path());
}

fn read_running_pid() -> Result<Option<u32>> {
    let path = pid_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let Ok(pid) = content.trim().parse::<u32>() else {
        clean_pid_file();
        return Ok(None);
    };

    if is_process_alive(pid) {
        Ok(Some(pid))
    } else {
        clean_pid_file();
        Ok(None)
    }
}

fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let pid_i32 = i32::try_from(pid).unwrap_or(0);
        // Safety: kill(pid, 0) only probes existence without sending a signal
        let ret = unsafe { libc::kill(pid_i32, 0) };
        ret == 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}
