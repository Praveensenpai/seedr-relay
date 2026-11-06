//! Background daemon lifecycle management and log attachment.

use crate::db::data_dir;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::time::sleep;

/// Active download progress persisted in `~/.local/share/seedr-relay/active.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveProgress {
    pub item_id: i64,
    pub name: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
    pub percentage: f64,
}

#[derive(Deserialize)]
struct CacheState {
    total_size: u64,
    chunks: Vec<CacheChunk>,
}

#[derive(Deserialize)]
struct CacheChunk {
    downloaded: u64,
}

/// Returns the path to the active download state file: `~/.local/share/seedr-relay/active.json`.
#[must_use]
pub fn active_path() -> PathBuf {
    data_dir().join("active.json")
}

/// Persists the active download progress to disk.
pub fn save_active_progress(progress: &ActiveProgress) {
    let path = active_path();
    if let Ok(json) = serde_json::to_string(progress) {
        let _ = fs::write(path, json);
    }
}

/// Loads active download progress if a download is in flight.
#[must_use]
pub fn load_active_progress() -> Option<ActiveProgress> {
    let path = active_path();
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(prog) = serde_json::from_str::<ActiveProgress>(&content) {
            return Some(prog);
        }
    }
    load_fallback_progress_from_cache()
}

fn load_fallback_progress_from_cache() -> Option<ActiveProgress> {
    let cache_dir = dirs::cache_dir()?.join("seedr-dl/downloads");
    if !cache_dir.is_dir() {
        return None;
    }

    for entry in fs::read_dir(cache_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let name_str = path.file_name()?.to_str()?;
        if name_str.ends_with(".state.json") {
            let content = fs::read_to_string(&path).ok()?;
            let state: CacheState = serde_json::from_str(&content).ok()?;
            let downloaded_bytes: u64 = state.chunks.iter().map(|c| c.downloaded).sum();
            #[allow(clippy::cast_precision_loss)]
            let percentage = if state.total_size > 0 {
                (downloaded_bytes as f64 / state.total_size as f64) * 100.0
            } else {
                0.0
            };
            let file_stem = name_str.trim_end_matches(".state.json").to_string();

            return Some(ActiveProgress {
                item_id: 0,
                name: file_stem,
                downloaded_bytes,
                total_bytes: state.total_size,
                speed_bps: 0,
                eta_seconds: 0,
                percentage,
            });
        }
    }
    None
}

/// Clears the active download state.
pub fn clear_active_progress() {
    let _ = fs::remove_file(active_path());
}

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

    let mut cmd = Command::new(exe);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(stderr_file));

    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let child = cmd.spawn().context("failed to spawn daemon process")?;

    let pid = child.id();
    fs::write(pid_path(), pid.to_string())
        .context("failed to write daemon PID file")?;

    println!("  {} Seedr Relay daemon started (PID: {})", "✔".green().bold(), pid);
    println!("  • View live logs : {}", "seedr-relay attach".cyan());
    println!("  • Stop daemon    : {}", "seedr-relay stop".cyan());
    Ok(())
}

/// Attaches to live daemon output, streaming newly appended lines and live progress until Ctrl+C.
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
        "  {} Attaching to daemon output {}... (Press Ctrl+C to detach)\n",
        "==>".cyan().bold(),
        pid_info
    );

    let mut file = File::open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    let meta = file.metadata()?;
    let start_pos = meta.len().saturating_sub(4096);
    file.seek(SeekFrom::Start(start_pos))?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut last_progress_drawn = false;

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                handle_idle(&mut last_progress_drawn);
                sleep(Duration::from_millis(250)).await;
            }
            Ok(_) => {
                handle_line(&line, &mut last_progress_drawn);
            }
            Err(e) => {
                eprintln!("Error reading log: {e}");
                break;
            }
        }
    }
    Ok(())
}

fn handle_idle(last_progress_drawn: &mut bool) {
    if let Some(p) = load_active_progress() {
        render_progress_bar(&p);
        *last_progress_drawn = true;
    } else if *last_progress_drawn {
        print!("\r\x1b[K");
        let _ = std::io::stdout().flush();
        *last_progress_drawn = false;
    }
}

fn handle_line(line: &str, last_progress_drawn: &mut bool) {
    if *last_progress_drawn {
        print!("\r\x1b[K");
        *last_progress_drawn = false;
    }
    print!("{line}");
    let _ = std::io::stdout().flush();
}

fn render_progress_bar(p: &ActiveProgress) {
    #[allow(clippy::cast_precision_loss)]
    let total_gb = p.total_bytes as f64 / 1_073_741_824.0;
    #[allow(clippy::cast_precision_loss)]
    let dl_gb = p.downloaded_bytes as f64 / 1_073_741_824.0;

    let width = 24_usize;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let filled = ((p.percentage / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let bar = format!("{}{}", "━".repeat(filled).cyan(), "─".repeat(width - filled).dimmed());

    let short_title = if p.name.len() > 28 {
        format!("{}...", &p.name[..25])
    } else {
        p.name.clone()
    };
    let title_prefix = if p.item_id > 0 {
        format!("[{}] {short_title}", p.item_id)
    } else {
        short_title
    };

    if p.speed_bps > 0 {
        #[allow(clippy::cast_precision_loss)]
        let speed_mb = p.speed_bps as f64 / 1_048_576.0;
        let eta_m = p.eta_seconds / 60;
        let eta_s = p.eta_seconds % 60;
        print!(
            "\r\x1b[K  {} [{bar}] {:5.1}% ({dl_gb:.2}/{total_gb:.2} GiB) • {:>4.1} MB/s • ETA {eta_m}m {eta_s:02}s",
            title_prefix.dimmed(), p.percentage, speed_mb
        );
    } else {
        print!(
            "\r\x1b[K  {} [{bar}] {:5.1}% ({dl_gb:.2}/{total_gb:.2} GiB)",
            title_prefix.dimmed(), p.percentage
        );
    }
    let _ = std::io::stdout().flush();
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
