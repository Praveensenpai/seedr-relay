# CODEBASE.md: seedr-relay Semantic Digest

> **Notice**: AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow
```text
Entrypoint ──> CLI/Parser ──> Domain Logic ──> Infra/IO
```

## 2. Global Constraints & Architecture Patterns
- **Primary Language**: Rust 2021 edition
- **Architectural Paradigm**: Role-based (domain/, infra/, api/cli/, tui/)
- **Hard Constraints**: <400 lines/file, <60 lines/fn, zero production unwrap(), 0 warnings.
- **Target Distribution**: Linux x86_64 standalone binary

## 3. Module & Interface Skeleton

### `src/cleaner.rs` (Role: general, Lines: 197)
- **Responsibility**: Core general logic in src/cleaner.rs
- **Imports**: use anyhow :: { Context , Result } , use chrono :: Local , use regex :: Regex , use std :: fmt :: Write as _ , use std :: fs , use std :: path :: { Path , PathBuf } 
- **Types & Enums**:
  ```rust
  pub struct RenameRecord
  pub struct SanitizationResult
  ```
- **Public Functions & Signatures**:
  ```rust
  fn clean_title (raw : & str) -> String
  fn clean_filename (filename : & str) -> String
  fn sanitize_folder (folder_path : & Path , original_folder_name : & str , magnet : & str ,) -> Result < SanitizationResult >
  ```

### `src/commands.rs` (Role: general, Lines: 157)
- **Responsibility**: Core general logic in src/commands.rs
- **Imports**: use crate :: config :: { load_config , save_config } , use crate :: db :: { QueueDb , QueueStatus } , use crate :: worker , use anyhow :: { Context , Result } , use colored :: Colorize , use std :: path :: { Path , PathBuf } 
- **Public Functions & Signatures**:
  ```rust
  fn handle_add (magnets : & [String] , output : Option < & Path >) -> Result < () >
  async fn handle_run (watch : bool , output : Option < PathBuf >) -> Result < () >
  fn handle_list (limit : usize) -> Result < () >
  fn handle_remove (id : i64) -> Result < () >
  fn handle_clean () -> Result < () >
  fn handle_config (save_path : Option < PathBuf > , notify : Option < bool > , show : bool ,) -> Result < () >
  ```

### `src/config.rs` (Role: general, Lines: 101)
- **Responsibility**: Core general logic in src/config.rs
- **Imports**: use anyhow :: { Context , Result } , use serde :: { Deserialize , Serialize } , use std :: fs , use std :: path :: PathBuf 
- **Types & Enums**:
  ```rust
  pub struct Config
  ```
- **Public Functions & Signatures**:
  ```rust
  fn config_dir () -> PathBuf
  fn config_path () -> PathBuf
  fn default_save_path () -> PathBuf
  fn load_config () -> Result < Config >
  fn save_config (config : & Config) -> Result < () >
  ```

### `src/db.rs` (Role: general, Lines: 267)
- **Responsibility**: Core general logic in src/db.rs
- **Imports**: use anyhow :: { Context , Result } , use chrono :: Utc , use rusqlite :: { params , Connection } , use std :: fmt , use std :: fs , use std :: path :: { Path , PathBuf } 
- **Types & Enums**:
  ```rust
  pub enum QueueStatus
  pub struct QueueItem
  pub struct QueueDb
  ```
- **Public Functions & Signatures**:
  ```rust
  fn as_str (self) -> & 'static str
  fn parse (s : & str) -> Self
  fn open () -> Result < Self >
  fn enqueue (& self , magnet : & str , output_dir : Option < & Path >) -> Result < QueueItem >
  fn next_pending (& self) -> Result < Option < QueueItem > >
  fn update_status (& self , id : i64 , status : QueueStatus , error : Option < & str >) -> Result < () >
  fn update_name (& self , id : i64 , name : & str) -> Result < () >
  fn list_items (& self , limit : usize) -> Result < Vec < QueueItem > >
  fn remove_item (& self , id : i64) -> Result < bool >
  fn clean_completed (& self) -> Result < usize >
  fn data_dir () -> PathBuf
  ```

### `src/main.rs` (Role: general, Lines: 116)
- **Responsibility**: Core general logic in src/main.rs
- **Imports**: use anyhow :: Result , use clap :: { Parser , Subcommand } , use std :: path :: PathBuf 

### `src/notify.rs` (Role: general, Lines: 33)
- **Responsibility**: Core general logic in src/notify.rs
- **Imports**: use anyhow :: { bail , Context , Result } , use tokio :: process :: Command 
- **Public Functions & Signatures**:
  ```rust
  async fn done (title : & str , body : & str)
  async fn alert (title : & str , body : & str)
  ```

### `src/worker.rs` (Role: general, Lines: 164)
- **Responsibility**: Core general logic in src/worker.rs
- **Imports**: use crate :: cleaner , use crate :: config :: Config , use crate :: db :: { QueueDb , QueueItem , QueueStatus } , use crate :: notify , use anyhow :: { Context , Result } , use colored :: Colorize , use seedr_dl :: seedr :: { CachingQuery , SeedrClient , SeedrFolder } , use std :: fs , use std :: path :: { Path , PathBuf } , use std :: time :: Duration , use tokio :: time :: sleep 
- **Public Functions & Signatures**:
  ```rust
  async fn process_queue (config : & Config , db : & QueueDb , watch : bool) -> Result < () >
  ```

## 4. Execution Lifecycle Trace
1. **Startup**: Entrypoint parses CLI flags & dispatches command.
2. **Execution**: Core domain logic processes inputs and evaluates rules.
3. **Persistence / I/O**: Domain logic calls infra for disk/terminal I/O.
4. **Exit**: Graceful termination with standard exit codes.

## 5. Verification Commands
```bash
cargo build --release --target x86_64-unknown-linux-gnu
cargo test --all-targets
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
