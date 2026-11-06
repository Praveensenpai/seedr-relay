<div align="center">

# ⚡ seedr-relay
### 連係 — Sequential Cloud Ingestion, Local Downloader & Title Sanitizer

[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Engine: seedr-dl](https://img.shields.io/badge/Engine-seedr--dl-green.svg?style=flat-square)](https://github.com/Praveensenpai/seedr-dl)
[![Notify: Tayori](https://img.shields.io/badge/Notifications-Tayori-purple.svg?style=flat-square)](https://github.com/Praveensenpai/tayori)

<p align="center">
  <b>A resilient queue daemon and automated media sanitizer built on top of <code>seedr-dl</code>.</b><br>
  Dynamically add magnet links into a FIFO queue. Automatically stages each torrent to Seedr.cc, notifies your phone, downloads to local storage, purges cloud quota, sanitizes messy filenames, and generates an audit <code>note.txt</code>.
</p>

</div>

---

## 🌊 Architecture & Lifecycle

```
[seedr-relay add <magnet>] ──> SQLite Queue (~/.local/share/seedr-relay/queue.db)
                                     │
                 ┌───────────────────┴───────────────────┐
                 ▼                                       │
        1. Fetch Next Magnet                             │
                 │                                       │
                 ▼                                       │
        2. Push to Seedr & Wait for Cache                │
                 │                                       │
                 ▼                                       │
        3. 🔔 Notify (Tayori): "Seedr Cached"            │ (Add magnets anytime
                 │                                       │  in another terminal)
                 ▼                                       │
        4. Download to Local Destination                 │
                 │                                       │
                 ▼                                       │
        5. 🔔 Notify (Tayori): "Downloaded to Local"     │
                 │                                       │
                 ▼                                       │
        6. Delete Item from Seedr Cloud (Free Quota)     │
                 │                                       │
                 ▼                                       │
        7. Sanitize Filenames & Generate `note.txt`      │
                 │                                       │
                 ▼                                       │
        8. Mark Done in Queue ──> Loop to Step 1 ────────┘
```

---

## ✨ Features

- **Dynamic FIFO Queue**: Add magnets anytime while the downloader is running.
- **Two-Stage Mobile Notifications**: Real-time Telegram alerts via `tayori` when caching in Seedr finishes and when local download finishes.
- **Quota Self-Healing**: Deletes completed folders from Seedr immediately after local download to free space for the next torrent in queue.
- **Automated Title Sanitization**: Strips messy resolution flags (`1080p`, `4k`), release groups (`[YTS.MX]`, `[SubsPlease]`), codecs (`x264`, `hevc`), and tracker tags.
- **Audit `note.txt`**: Records previous original filename and cleaned filename mapping inside every downloaded folder.
- **Customizable Save Path**: Configure default save path globally or override per command.

---

## 🚀 Quick Start & Usage

### 1. View / Configure Save Path
```bash
# View current configuration
seedr-relay config --show

# Change local download directory
seedr-relay config --save-path /home/paisen/Downloads/Torrents
```

### 2. Add Magnet Links to Queue
```bash
# Add a single magnet link
seedr-relay add "magnet:?xt=urn:btih:..."

# Or add directly without subcommand
seedr-relay "magnet:?xt=urn:btih:..."

# Add with a custom one-off output directory
seedr-relay add "magnet:?xt=urn:btih:..." -o /mnt/storage/movies
```

### 3. Check Queue Status
```bash
seedr-relay list
```

### 4. Run the Sequential Worker
```bash
# Process all currently pending items and exit when queue is empty
seedr-relay run

# Or run in continuous watch mode (stays alive waiting for new magnets)
seedr-relay run --watch
```

### 5. Queue Management
```bash
# Remove an item by ID
seedr-relay remove 3

# Clear all completed items from queue history
seedr-relay clean
```

---

## 📝 Example `note.txt` Output

Inside each downloaded and cleaned folder, `seedr-relay` writes `note.txt`:

```text
================================================================================
                       SEEDR-RELAY INGESTION RECORD                             
================================================================================

Ingested At : 2026-11-06 15:45:00
Magnet Link : magnet:?xt=urn:btih:...

--------------------------------------------------------------------------------
PREVIOUS NAME -> CURRENT NAME
--------------------------------------------------------------------------------
  Prev: [DIR] Movie.Name.2024.1080p.WEBRip.x264-[YTS.MX]
  Curr: [DIR] Movie Name 2024

  Prev: Movie.Name.2024.1080p.WEBRip.x264-[YTS.MX].mp4
  Curr: Movie Name 2024.mp4

================================================================================
```
