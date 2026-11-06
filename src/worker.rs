//! Sequential download worker processing the magnet queue.

use crate::cleaner;
use crate::config::Config;
use crate::db::{QueueDb, QueueItem, QueueStatus};
use crate::notify;
use anyhow::{Context, Result};
use colored::Colorize;
use seedr_dl::seedr::{CachingQuery, SeedrClient, SeedrFolder};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

/// Processes the queue sequentially until empty (or continuously if watching).
///
/// # Errors
/// Returns an error on unrecoverable authentication or database errors.
pub async fn process_queue(config: &Config, db: &QueueDb, watch: bool) -> Result<()> {
    println!("{}", "==> Seedr Relay Worker Started".cyan().bold());

    loop {
        match db.next_pending()? {
            Some(item) => {
                println!("{} [{}] {}", "-> Processing:".yellow().bold(), item.id, item.name);
                if let Err(err) = process_item(config, db, &item).await {
                    eprintln!("{} Failed [{}]: {err:#}", "✖".red(), item.id);
                    db.update_status(item.id, QueueStatus::Failed, Some(&format!("{err:#}")))?;
                    if config.notify {
                        notify::alert("Seedr Relay Failed", &format!("{}: {err:#}", item.name)).await;
                    }
                }
            }
            None => {
                if watch {
                    sleep(Duration::from_secs(config.poll_interval_secs)).await;
                } else {
                    println!("{}", "✔ Queue is empty. All items finished.".green().bold());
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn process_item(config: &Config, db: &QueueDb, item: &QueueItem) -> Result<()> {
    db.update_status(item.id, QueueStatus::Caching, None)?;

    let client = get_seedr_client()?;
    let folder = wait_and_cache_magnet(&client, &item.magnet).await?;
    db.update_name(item.id, &folder.name)?;

    let sz_mb = folder.size.unwrap_or(0) / 1_048_576;
    println!("  {} Cached in Seedr cloud ({} MB)", "✔".green(), sz_mb);

    if config.notify {
        let msg = format!("Torrent: {}\nSize: {} MB\nCached in Seedr cloud. Starting local download...", folder.name, sz_mb);
        notify::done("Seedr Cached Magnet", &msg).await;
    }

    db.update_status(item.id, QueueStatus::Downloading, None)?;
    let output_dir = item.output_dir.clone().unwrap_or_else(|| config.save_path.clone());
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create output directory {}", output_dir.display()))?;

    let downloaded_files = download_from_cloud(&client, config, &folder, &output_dir).await?;

    if config.auto_clean_cloud {
        let _ = client.delete_folder(folder.id).await;
    }

    let sanitized = post_process_downloads(&output_dir, &folder.name, &downloaded_files, &item.magnet)?;
    println!(
        "  {} Saved & sanitized to: {} (Note: {})",
        "✔".green(),
        sanitized.target_dir.display(),
        sanitized.note_path.display()
    );

    if config.notify {
        let note_summary = format!(
            "Name: {}\nCleaned: {}\nSaved: {}\nRenamed {} items (note.txt generated)\nSeedr cloud space reclaimed.",
            folder.name,
            sanitized.target_dir.file_name().and_then(|n| n.to_str()).unwrap_or(&folder.name),
            sanitized.target_dir.display(),
            sanitized.records.len()
        );
        notify::done("Downloaded to Local", &note_summary).await;
    }

    db.update_status(item.id, QueueStatus::Completed, None)?;
    Ok(())
}

fn get_seedr_client() -> Result<SeedrClient> {
    let auth = seedr_dl::load_auth()?
        .context("Seedr credentials missing. Please run `seedr-dl auth` to log in")?;
    Ok(SeedrClient::new(auth.access_token))
}

async fn wait_and_cache_magnet(client: &SeedrClient, magnet: &str) -> Result<SeedrFolder> {
    let root_before = client.list_root().await?;
    let prev_ids: Vec<u64> = root_before.folders.iter().map(|f| f.id).collect();
    let name_hint = seedr_dl::extract_magnet_name(magnet);

    let torrent_id = client.add_magnet(magnet).await?;
    let query = CachingQuery {
        torrent_id,
        name: name_hint.as_deref(),
        previous_folders: &prev_ids,
    };

    let folder_res = client.wait_for_caching(&query, None).await?;
    folder_res.context("Could not find cached folder in Seedr cloud")
}

async fn download_from_cloud(
    client: &SeedrClient,
    config: &Config,
    folder: &SeedrFolder,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let transfer = seedr_dl::TransferOptions {
        output_dir: output_dir.to_path_buf(),
        delete_cloud: config.auto_clean_cloud,
        notifier: None,
    };
    let seedr_cfg = seedr_dl::Config {
        download_dir: output_dir.to_path_buf(),
        delete_after_download: config.auto_clean_cloud,
        download_threads: 4,
    };

    seedr_dl::transfer_download_folder(client, &seedr_cfg, folder, &transfer).await
}

fn post_process_downloads(
    output_dir: &Path,
    original_name: &str,
    files: &[PathBuf],
    magnet: &str,
) -> Result<cleaner::SanitizationResult> {
    let multi_folder = output_dir.join(original_name);
    if multi_folder.is_dir() {
        return cleaner::sanitize_folder(&multi_folder, original_name, magnet);
    }

    let cleaned_name = cleaner::clean_title(original_name);
    let target_dir = output_dir.join(&cleaned_name);
    fs::create_dir_all(&target_dir)?;

    for file in files {
        if file.is_file() {
            let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            let target_file = target_dir.join(file_name);
            if *file != target_file {
                let _ = fs::rename(file, &target_file);
            }
        }
    }

    cleaner::sanitize_folder(&target_dir, original_name, magnet)
}
