//! Name sanitization and `note.txt` generation for downloaded torrents.

use anyhow::{Context, Result};
use chrono::Local;
use regex::Regex;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Record of file renaming for `note.txt`.
#[derive(Debug, Clone)]
pub struct RenameRecord {
    pub original_name: String,
    pub cleaned_name: String,
}

/// Result of sanitizing a downloaded folder or file collection.
#[derive(Debug, Clone)]
pub struct SanitizationResult {
    pub target_dir: PathBuf,
    pub records: Vec<RenameRecord>,
}

/// Cleans unwanted release tags, resolution flags, and watermarks from a title.
#[must_use]
pub fn clean_title(raw: &str) -> String {
    let mut title = raw.to_string();

    // Remove bracketed groups like [SubsPlease], [YTS.MX], [Erai-raws], [720p], [ABCD1234]
    if let Ok(bracket_re) = Regex::new(r"\[[^\]]+\]") {
        title = bracket_re.replace_all(&title, " ").to_string();
    }

    // Remove website domains like www.example.com, rarbg.to, yts.mx
    if let Ok(domain_re) = Regex::new(r"(?i)\bwww\.[a-z0-9-]+\.[a-z]{2,}\b|\b[a-z0-9-]+\.(?:com|org|net|to|mx|me|io)\b") {
        title = domain_re.replace_all(&title, " ").to_string();
    }

    // Remove junk quality / source / audio keywords
    let junk_pattern = r"(?i)\b(1080p|720p|480p|2160p|4k|uhd|bluray|blu-ray|webrip|web-dl|web|hdtv|hdrip|dvdrip|remux|x264|x265|h264|h265|hevc|avc|10bit|8bit|aac5\.1|aac2\.0|aac|ac3|dts|ddp5\.1|dual-audio|multi-audio)\b";
    if let Ok(junk_re) = Regex::new(junk_pattern) {
        title = junk_re.replace_all(&title, " ").to_string();
    }

    // Replace dots and underscores with spaces
    title = title.replace(['.', '_'], " ");

    // Remove empty parentheses or brackets left over
    if let Ok(empty_re) = Regex::new(r"\(\s*\)|\[\s*\]") {
        title = empty_re.replace_all(&title, " ").to_string();
    }

    // Collapse multiple spaces and trim
    if let Ok(space_re) = Regex::new(r"\s+") {
        title = space_re.replace_all(&title, " ").to_string();
    }

    let trimmed = title
        .trim()
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .trim();

    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Cleans a file name while preserving its file extension.
#[must_use]
pub fn clean_filename(filename: &str) -> String {
    let path = Path::new(filename);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(filename);
    let ext = path.extension().and_then(|e| e.to_str());

    let cleaned_stem = clean_title(stem);
    if let Some(extension) = ext {
        format!("{cleaned_stem}.{extension}")
    } else {
        cleaned_stem
    }
}

/// Sanitizes a downloaded folder, renames files inside, and writes `note.txt`.
///
/// # Errors
/// Returns an error if directory traversal, renaming, or writing `note.txt` fails.
pub fn sanitize_folder(
    folder_path: &Path,
    original_folder_name: &str,
    magnet: &str,
) -> Result<SanitizationResult> {
    let mut records = Vec::new();
    let cleaned_folder_name = clean_title(original_folder_name);

    let parent = folder_path.parent().unwrap_or_else(|| Path::new("."));
    let final_dir = if cleaned_folder_name == original_folder_name {
        folder_path.to_path_buf()
    } else {
        let dest = parent.join(&cleaned_folder_name);
        if dest != folder_path && !dest.exists() {
            fs::rename(folder_path, &dest)
                .with_context(|| format!("failed to rename folder to {}", dest.display()))?;
            dest
        } else {
            folder_path.to_path_buf()
        }
    };

    records.push(RenameRecord {
        original_name: format!("[DIR] {original_folder_name}"),
        cleaned_name: format!("[DIR] {cleaned_folder_name}"),
    });

    if final_dir.is_dir() {
        process_dir_entries(&final_dir, &mut records)?;
    }

    let note_path = final_dir.join("note.txt");
    write_note(&note_path, &records, magnet)?;

    Ok(SanitizationResult {
        target_dir: final_dir,
        records,
    })
}

fn process_dir_entries(dir: &Path, records: &mut Vec<RenameRecord>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            process_file_entry(dir, &path, records)?;
        }
    }
    Ok(())
}

fn process_file_entry(dir: &Path, path: &Path, records: &mut Vec<RenameRecord>) -> Result<()> {
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    if file_name == "note.txt" {
        return Ok(());
    }
    let cleaned = clean_filename(file_name);
    if cleaned != file_name {
        let target = dir.join(&cleaned);
        if !target.exists() {
            fs::rename(path, &target)?;
        }
    }
    records.push(RenameRecord {
        original_name: file_name.to_string(),
        cleaned_name: cleaned,
    });
    Ok(())
}

fn write_note(note_path: &Path, records: &[RenameRecord], magnet: &str) -> Result<()> {
    let mut content = String::new();
    content.push_str("================================================================================\n");
    content.push_str("                       SEEDR-RELAY INGESTION RECORD                             \n");
    content.push_str("================================================================================\n\n");
    let _ = writeln!(content, "Ingested At : {}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    let _ = writeln!(content, "Magnet Link : {magnet}\n");
    content.push_str("--------------------------------------------------------------------------------\n");
    content.push_str("PREVIOUS NAME -> CURRENT NAME\n");
    content.push_str("--------------------------------------------------------------------------------\n");

    for r in records {
        let _ = writeln!(content, "  Prev: {}\n  Curr: {}\n", r.original_name, r.cleaned_name);
    }

    content.push_str("================================================================================\n");
    fs::write(note_path, content)
        .with_context(|| format!("failed to write note at {}", note_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_title_removes_release_tags_and_resolutions() {
        let raw = "[SubsPlease] Frieren - 28 (1080p) [938210AB]";
        assert_eq!(clean_title(raw), "Frieren - 28");

        let movie = "Oppenheimer.2023.1080p.WEBRip.x264.AAC5.1-[YTS.MX]";
        assert_eq!(clean_title(movie), "Oppenheimer 2023");
    }

    #[test]
    fn clean_filename_preserves_extension() {
        let raw = "Series.Name.S01E02.720p.HDTV.x264.mkv";
        assert_eq!(clean_filename(raw), "Series Name S01E02.mkv");
    }
}
