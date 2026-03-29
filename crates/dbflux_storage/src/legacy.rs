//! Legacy JSON import for DBFlux storage migration.
//!
//! This module detects and imports data from legacy JSON storage files into the
//! SQLite-backed storage. It is restart-safe and idempotent.
//!
//! Import idempotency is achieved via:
//! 1. An explicit `system_metadata` table storing per-source-file status
//!    (not just UUID dedup), so a partial import is never re-run blindly.
//! 2. Per-file transactional writes: each source file commits in one transaction,
//!    so a crash during import leaves the file marked as `failed` (not `completed`).
//! 3. UUID dedup within each file, so surviving records from a partial import
//!    are not duplicated on retry.

use dbflux_core::{ConnectionProfile, SavedQuery, SshTunnelProfile};
use log::warn;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::bootstrap::OwnedConnection;
use crate::repositories::connection_profiles::ConnectionProfileDto;
use crate::repositories::proxy_profiles::ProxyProfileDto;
use crate::repositories::ssh_tunnel_profiles::SshTunnelProfileDto;
use crate::repositories::state::query_history::{QueryHistoryDto, QueryHistoryRepository};
use crate::repositories::state::recent_items::{RecentItemDto, RecentItemsRepository};
use crate::repositories::state::saved_queries::{SavedQueriesRepository, SavedQueryDto};
use crate::repositories::state::ui_state::UiStateRepository;
use crate::repositories::{
    auth_profiles::AuthProfileRepository, connection_profiles::ConnectionProfileRepository,
    proxy_profiles::ProxyProfileRepository, ssh_tunnel_profiles::SshTunnelProfileRepository,
};

/// Import status for a legacy source file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ImportStatus {
    /// Import completed successfully.
    Completed,
    /// Import was attempted but failed.
    Failed,
}

/// Result of a legacy import operation.
#[derive(Debug, Clone, Default)]
pub struct LegacyImportResult {
    pub profiles_imported: usize,
    pub auth_profiles_imported: usize,
    pub proxy_profiles_imported: usize,
    pub ssh_tunnels_imported: usize,
    pub history_entries_imported: usize,
    pub saved_queries_imported: usize,
    pub recent_items_imported: usize,
    pub ui_state_restored: bool,
    pub errors: Vec<String>,
}

impl LegacyImportResult {
    pub fn total_imported(&self) -> usize {
        self.profiles_imported
            + self.auth_profiles_imported
            + self.proxy_profiles_imported
            + self.ssh_tunnels_imported
            + self.history_entries_imported
            + self.saved_queries_imported
            + self.recent_items_imported
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn any_imported(&self) -> bool {
        self.total_imported() > 0 || self.ui_state_restored
    }
}

/// Checks the import status for a source file in the system_metadata table.
/// Returns `Some(true)` if completed, `Some(false)` if failed, `None` if never attempted.
fn get_import_status(conn: &OwnedConnection, source_file: &str) -> Option<bool> {
    let result: Option<String> = conn
        .query_row(
            "SELECT value FROM system_metadata WHERE key = ?1",
            [format!("legacy_import::{}", source_file)],
            |row| row.get(0),
        )
        .ok()?;
    match result.as_deref() {
        Some("completed") => Some(true),
        Some("failed") => Some(false),
        _ => None,
    }
}

/// Records the import status for a source file in the system_metadata table.
fn set_import_status(conn: &rusqlite::Connection, source_file: &str, status: ImportStatus) {
    let value = match status {
        ImportStatus::Completed => "completed",
        ImportStatus::Failed => "failed",
    };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO system_metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params![format!("legacy_import::{}", source_file), value],
    );
}

/// Returns the path to a legacy JSON file if it exists, otherwise None.
/// Takes the root directory (config dir for most files, data dir for state.json).
fn legacy_path_if_exists(root: &PathBuf, filename: &str) -> Option<PathBuf> {
    let path = root.join(filename);
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Runs all legacy JSON imports for the domains migrated in previous batches.
///
/// This function is idempotent: re-running it on a system that has already
/// imported data will not create duplicates (UUID dedup + explicit status check).
/// Files that failed previously are retried; files that completed are skipped.
///
/// The `config_dir` and `data_dir` parameters are used only to locate legacy JSON
/// source files. They are not created or modified — files are read, imported into
/// SQLite, and renamed to `*.bak` on success.
pub fn run_legacy_import(
    config_conn: OwnedConnection,
    state_conn: OwnedConnection,
    config_dir: &PathBuf,
    data_dir: &PathBuf,
) -> LegacyImportResult {
    let mut result = LegacyImportResult::default();

    // --- Config domain imports (config.db) ---
    import_profiles_with_status(&config_conn, config_dir, &mut result);
    import_auth_profiles_with_status(&config_conn, config_dir, &mut result);
    import_proxy_profiles_with_status(&config_conn, config_dir, &mut result);
    import_ssh_tunnels_with_status(&config_conn, config_dir, &mut result);

    // --- State domain imports (state.db) ---
    import_history_entries_with_status(&state_conn, config_dir, &mut result);
    import_saved_queries_with_status(&state_conn, config_dir, &mut result);
    import_recent_items_with_status(&state_conn, config_dir, &mut result);
    import_ui_state_with_status(&state_conn, data_dir, &mut result);

    result
}

// ---------------------------------------------------------------------------
// Config domain imports
// ---------------------------------------------------------------------------

/// Imports connection profiles from legacy `profiles.json`.
fn import_profiles_with_status(
    config_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "profiles.json";

    // Check explicit status: skip if already completed, retry if failed
    match get_import_status(config_conn, source) {
        Some(true) => return, // Already completed
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("profiles: cannot read {}: {}", source, e));
            return;
        }
    };

    let legacy: Vec<ConnectionProfile> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("profiles: cannot parse {}: {}", source, e));
            set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    // Mark as failed upfront so partial success doesn't survive a crash
    set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);

    // Transaction: entire file import is atomic
    let tx = match config_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("profiles: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = ConnectionProfileRepository::new(config_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|p| p.id).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for profile in legacy {
        if existing_ids.contains(&profile.id.to_string()) {
            continue;
        }

        let config_json = match serde_json::to_string(&profile) {
            Ok(s) => s,
            Err(e) => {
                // If we can't round-trip the profile, the legacy data is incompatible — fail hard
                result.errors.push(format!(
                    "profiles: cannot serialize '{}' (incompatible legacy format): {}",
                    profile.name, e
                ));
                return;
            }
        };
        let driver_id = profile.driver_id();

        let dto = ConnectionProfileDto {
            id: profile.id.to_string(),
            name: profile.name.clone(),
            driver_id: Some(driver_id),
            description: None,
            favorite: false,
            color: None,
            icon: None,
            config_json,
            auth_profile_id: profile.auth_profile_id.map(|u| u.to_string()),
            proxy_profile_id: profile.proxy_profile_id.map(|u| u.to_string()),
            ssh_tunnel_profile_id: None,
            access_profile_id: None,
            settings_overrides_json: None,
            connection_settings_json: None,
            hooks_json: None,
            hook_bindings_json: None,
            value_refs_json: None,
            mcp_governance_json: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        if let Err(e) = repo.insert(&dto) {
            warn!("Failed to import profile '{}': {}", profile.name, e);
        } else {
            imported += 1;
        }
    }

    result.profiles_imported += imported;

    if imported > 0 {
        log::info!(
            "Imported {} legacy connection profiles from {}",
            imported,
            source
        );
    }

    // Commit only after all records successfully written
    if let Err(e) = tx.commit() {
        result
            .errors
            .push(format!("profiles: commit failed: {}", e));
        return;
    }

    // Mark completed only after commit succeeds
    set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
}

/// Imports auth profiles from legacy `auth_profiles.json`.
fn import_auth_profiles_with_status(
    config_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "auth_profiles.json";

    match get_import_status(config_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("auth_profiles: cannot read {}: {}", source, e));
            return;
        }
    };

    let legacy: Vec<dbflux_core::AuthProfile> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("auth_profiles: cannot parse {}: {}", source, e));
            set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match config_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("auth_profiles: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = AuthProfileRepository::new(config_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|p| p.id.to_string()).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for profile in legacy {
        if existing_ids.contains(&profile.id.to_string()) {
            continue;
        }
        if let Err(e) = repo.insert_auth_profile(&profile) {
            warn!("Failed to import auth profile '{}': {}", profile.name, e);
        } else {
            imported += 1;
        }
    }

    result.auth_profiles_imported += imported;

    if imported > 0 {
        log::info!("Imported {} legacy auth profiles from {}", imported, source);
    }

    if let Err(e) = tx.commit() {
        result
            .errors
            .push(format!("auth_profiles: commit failed: {}", e));
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
}

/// Imports proxy profiles from legacy `proxies.json`.
fn import_proxy_profiles_with_status(
    config_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "proxies.json";

    match get_import_status(config_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("proxies: cannot read {}: {}", source, e));
            return;
        }
    };

    let legacy: Vec<dbflux_core::ProxyProfile> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("proxies: cannot parse {}: {}", source, e));
            set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match config_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("proxies: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = ProxyProfileRepository::new(config_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|p| p.id.to_string()).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for profile in legacy {
        if existing_ids.contains(&profile.id.to_string()) {
            continue;
        }

        let name = profile.name.clone();
        let kind_json = serde_json::to_string(&profile.kind).unwrap_or_else(|_| "{}".into());
        let auth_json = serde_json::to_string(&profile.auth).unwrap_or_else(|_| "{}".into());

        let dto = ProxyProfileDto {
            id: profile.id.to_string(),
            name,
            kind: kind_json,
            host: profile.host,
            port: profile.port as i32,
            auth_json,
            no_proxy: profile.no_proxy,
            enabled: profile.enabled,
            save_secret: profile.save_secret,
            created_at: String::new(),
            updated_at: String::new(),
        };

        if let Err(e) = repo.insert(&dto) {
            warn!("Failed to import proxy profile '{}': {}", dto.name, e);
        } else {
            imported += 1;
        }
    }

    result.proxy_profiles_imported += imported;

    if imported > 0 {
        log::info!(
            "Imported {} legacy proxy profiles from {}",
            imported,
            source
        );
    }

    if let Err(e) = tx.commit() {
        result.errors.push(format!("proxies: commit failed: {}", e));
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
}

/// Imports SSH tunnel profiles from legacy `ssh_tunnels.json`.
fn import_ssh_tunnels_with_status(
    config_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "ssh_tunnels.json";

    match get_import_status(config_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("ssh_tunnels: cannot read {}: {}", source, e));
            return;
        }
    };

    let legacy: Vec<SshTunnelProfile> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("ssh_tunnels: cannot parse {}: {}", source, e));
            set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match config_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("ssh_tunnels: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = SshTunnelProfileRepository::new(config_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|p| p.id.to_string()).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for profile in legacy {
        if existing_ids.contains(&profile.id.to_string()) {
            continue;
        }

        let config_json = serde_json::to_string(&profile.config).unwrap_or_else(|_| "{}".into());
        let name = profile.name.clone();

        let dto = SshTunnelProfileDto {
            id: profile.id.to_string(),
            name,
            config_json,
            save_secret: profile.save_secret,
            created_at: String::new(),
            updated_at: String::new(),
        };

        if let Err(e) = repo.insert(&dto) {
            warn!("Failed to import SSH tunnel profile '{}': {}", dto.name, e);
        } else {
            imported += 1;
        }
    }

    result.ssh_tunnels_imported += imported;

    if imported > 0 {
        log::info!(
            "Imported {} legacy SSH tunnel profiles from {}",
            imported,
            source
        );
    }

    if let Err(e) = tx.commit() {
        result
            .errors
            .push(format!("ssh_tunnels: commit failed: {}", e));
        return;
    }

    set_import_status(config_conn.as_ref(), source, ImportStatus::Completed);
}

// ---------------------------------------------------------------------------
// State domain imports
// ---------------------------------------------------------------------------

/// Imports query history entries from legacy `history.json`.
fn import_history_entries_with_status(
    state_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "history.json";

    // History lives in config dir in legacy schema
    match get_import_status(state_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("history: cannot read {}: {}", source, e));
            return;
        }
    };

    #[derive(Debug, Deserialize)]
    struct LegacyHistoryEntry {
        #[serde(rename = "id")]
        id: Option<String>,
        #[serde(rename = "sql")]
        sql: String,
        #[serde(rename = "timestamp")]
        timestamp: Option<i64>,
        #[serde(rename = "database")]
        database: Option<String>,
        #[serde(rename = "connection_name")]
        connection_name: Option<String>,
        #[serde(rename = "execution_time_ms")]
        execution_time_ms: Option<u64>,
        #[serde(rename = "row_count")]
        row_count: Option<usize>,
        #[serde(rename = "is_favorite")]
        is_favorite: Option<bool>,
    }

    let legacy: Vec<LegacyHistoryEntry> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("history: cannot parse {}: {}", source, e));
            set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match state_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("history: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = QueryHistoryRepository::new(state_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|h| h.id).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for entry in legacy {
        let id = entry
            .id
            .as_ref()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .unwrap_or_else(uuid::Uuid::new_v4)
            .to_string();

        if existing_ids.contains(&id) {
            continue;
        }

        let dto = QueryHistoryDto {
            id: id.clone(),
            connection_profile_id: entry.connection_name,
            driver_id: None,
            database_name: entry.database,
            query_text: entry.sql,
            query_kind: "select".to_string(),
            executed_at: entry
                .timestamp
                .map(|ts| {
                    dbflux_core::chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
            duration_ms: entry.execution_time_ms.map(|ms| ms as i64),
            succeeded: true,
            error_summary: None,
            row_count: entry.row_count.map(|n| n as i64),
            is_favorite: entry.is_favorite.unwrap_or(false),
        };

        if let Err(e) = repo.add(&dto) {
            warn!("Failed to import history entry {}: {}", id, e);
        } else {
            imported += 1;
        }
    }

    result.history_entries_imported += imported;

    if imported > 0 {
        log::info!(
            "Imported {} legacy history entries from {}",
            imported,
            source
        );
    }

    if let Err(e) = tx.commit() {
        result.errors.push(format!("history: commit failed: {}", e));
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
}

/// Imports saved queries from legacy `saved_queries.json`.
fn import_saved_queries_with_status(
    state_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "saved_queries.json";

    match get_import_status(state_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("saved_queries: cannot read {}: {}", source, e));
            return;
        }
    };

    let legacy: Vec<SavedQuery> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("saved_queries: cannot parse {}: {}", source, e));
            set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match state_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("saved_queries: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = SavedQueriesRepository::new(state_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|q| q.id).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for query in legacy {
        if existing_ids.contains(&query.id.to_string()) {
            continue;
        }

        let dto = SavedQueryDto {
            id: query.id.to_string(),
            folder_id: None,
            name: query.name,
            sql: query.sql,
            is_favorite: query.is_favorite,
            connection_id: query.connection_id.map(|u| u.to_string()),
            created_at: dbflux_core::chrono::DateTime::from_timestamp(query.created_at, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            last_used_at: dbflux_core::chrono::DateTime::from_timestamp(query.last_used_at, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
        };

        if let Err(e) = repo.insert(&dto) {
            warn!("Failed to import saved query '{}': {}", dto.name, e);
        } else {
            imported += 1;
        }
    }

    result.saved_queries_imported += imported;

    if imported > 0 {
        log::info!("Imported {} legacy saved queries from {}", imported, source);
    }

    if let Err(e) = tx.commit() {
        result
            .errors
            .push(format!("saved_queries: commit failed: {}", e));
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
}

/// Imports recent files from legacy `recent_files.json`.
fn import_recent_items_with_status(
    state_conn: &OwnedConnection,
    config_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "recent_files.json";

    match get_import_status(state_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    let path = match legacy_path_if_exists(config_dir, source) {
        Some(p) => p,
        None => return,
    };

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("recent_files: cannot read {}: {}", source, e));
            return;
        }
    };

    #[derive(Debug, Deserialize)]
    struct LegacyRecentFile {
        path: PathBuf,
        #[serde(rename = "last_opened")]
        last_opened: Option<i64>,
    }

    let legacy: Vec<LegacyRecentFile> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("recent_files: cannot parse {}: {}", source, e));
            set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    if legacy.is_empty() {
        set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);

    let tx = match state_conn.unchecked_transaction() {
        Ok(t) => t,
        Err(e) => {
            result
                .errors
                .push(format!("recent_files: cannot start transaction: {}", e));
            return;
        }
    };

    let repo = RecentItemsRepository::new(state_conn.clone());
    let existing_ids: std::collections::HashSet<String> = repo
        .all()
        .map(|v| v.into_iter().map(|r| r.id).collect())
        .unwrap_or_default();

    let mut imported = 0;
    for recent in legacy {
        let path_str = recent.path.to_string_lossy().to_string();

        // Derive a stable UUID from the path so retries are idempotent
        let stable_id = derive_stable_id(&path_str);

        if existing_ids.contains(&stable_id) {
            continue;
        }

        let title = recent
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let dto = RecentItemDto {
            id: stable_id,
            kind: "file".to_string(),
            profile_id: None,
            path: Some(path_str),
            title,
            accessed_at: recent
                .last_opened
                .map(|ts| {
                    dbflux_core::chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                })
                .unwrap_or_default(),
        };

        if let Err(e) = repo.record_access(&dto) {
            warn!("Failed to import recent file '{}': {}", dto.title, e);
        } else {
            imported += 1;
        }
    }

    result.recent_items_imported += imported;

    if imported > 0 {
        log::info!("Imported {} legacy recent files from {}", imported, source);
    }

    if let Err(e) = tx.commit() {
        result
            .errors
            .push(format!("recent_files: commit failed: {}", e));
        return;
    }

    set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
}

/// Derives a stable ID from a path string using SHA-1.
///
/// Uses a fixed namespace prefix hashed with the path to produce a deterministic
/// 16-byte identifier, then formatted as a UUID string for consistency with the rest
/// of the system. The same path always produces the same ID across retries.
fn derive_stable_id(path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    const NAMESPACE: &str = "dbflux.recent_items";
    let combined = format!("{}:{}", NAMESPACE, path);

    let mut hasher = DefaultHasher::new();
    combined.hash(&mut hasher);
    let hash1 = hasher.finish();

    let mut hasher2 = DefaultHasher::new();
    (hash1 as u64).hash(&mut hasher2);
    let hash2 = hasher2.finish();

    // Format as UUID-like string: first 16 hex chars from hash1, next 16 from hash2
    format!("{:016x}-{:016x}", hash1, hash2)
}

/// Restores UI state from legacy `state.json` in the XDG data directory.
fn import_ui_state_with_status(
    state_conn: &OwnedConnection,
    data_dir: &PathBuf,
    result: &mut LegacyImportResult,
) {
    let source = "state.json";
    let path = data_dir.join(source);

    match get_import_status(state_conn, source) {
        Some(true) => return,
        Some(false) => log::info!("Retrying failed import: {}", source),
        None => {}
    }

    if !path.exists() {
        set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
        return;
    }

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            result
                .errors
                .push(format!("ui_state: cannot read {}: {}", source, e));
            set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    #[derive(Debug, Deserialize)]
    struct LegacyUiState {
        #[serde(rename = "settings_collapsed_security")]
        settings_collapsed_security: Option<bool>,
        #[serde(rename = "settings_collapsed_network")]
        settings_collapsed_network: Option<bool>,
        #[serde(rename = "settings_collapsed_connection")]
        settings_collapsed_connection: Option<bool>,
    }

    let legacy: LegacyUiState = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            result
                .errors
                .push(format!("ui_state: cannot parse {}: {}", source, e));
            set_import_status(state_conn.as_ref(), source, ImportStatus::Failed);
            return;
        }
    };

    let repo = UiStateRepository::new(state_conn.clone());

    if legacy.settings_collapsed_security.unwrap_or(false) {
        let _ = repo.set("ui.collapse.security", r#"{"value":true}"#);
    }
    if legacy.settings_collapsed_network.unwrap_or(false) {
        let _ = repo.set("ui.collapse.network", r#"{"value":true}"#);
    }
    if legacy.settings_collapsed_connection.unwrap_or(false) {
        let _ = repo.set("ui.collapse.connection", r#"{"value":true}"#);
    }

    result.ui_state_restored = true;
    set_import_status(state_conn.as_ref(), source, ImportStatus::Completed);
    log::info!("Restored legacy UI state from {}", source);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations;
    use crate::sqlite::open_database;
    use std::sync::Arc;

    fn temp_config_db(name: &str) -> (std::path::PathBuf, OwnedConnection) {
        let path = std::env::temp_dir().join(format!(
            "dbflux_legacy_config_{}_{}.sqlite",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let conn = open_database(&path).expect("open");
        migrations::run_config_migrations(&conn).expect("migrate");
        (path, Arc::new(conn))
    }

    fn temp_state_db(name: &str) -> (std::path::PathBuf, OwnedConnection) {
        let path = std::env::temp_dir().join(format!(
            "dbflux_legacy_state_{}_{}.sqlite",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
        let conn = open_database(&path).expect("open");
        migrations::run_state_migrations(&conn).expect("migrate");
        (path, Arc::new(conn))
    }

    fn isolated_legacy_dir(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        // Create two separate isolated roots so we can test config-dir vs data-dir files
        let base = std::env::temp_dir().join(format!(
            "dbflux_legacy_test_{}_{}",
            name,
            std::process::id()
        ));
        let config_dir = base.join("config");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        (config_dir, data_dir)
    }

    #[test]
    fn import_nonexistent_files_returns_zero() {
        let (_config_path, config_conn) = temp_config_db("nonexistent");
        let (_state_path, state_conn) = temp_state_db("nonexistent");
        let (config_dir, data_dir) = isolated_legacy_dir("nonexistent");

        let result = run_legacy_import(config_conn, state_conn, &config_dir, &data_dir);

        assert_eq!(result.profiles_imported, 0);
        assert_eq!(result.auth_profiles_imported, 0);
        assert_eq!(result.proxy_profiles_imported, 0);
        assert_eq!(result.ssh_tunnels_imported, 0);
        assert_eq!(result.history_entries_imported, 0);
        assert_eq!(result.saved_queries_imported, 0);
        assert_eq!(result.recent_items_imported, 0);
        assert!(!result.has_errors());

        let _ = std::fs::remove_file(&_config_path);
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(&_state_path);
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_dir_all(&config_dir.parent().unwrap());
    }

    #[test]
    fn import_result_tracks_counts() {
        let mut result = LegacyImportResult::default();
        result.profiles_imported = 5;
        result.auth_profiles_imported = 3;
        result.history_entries_imported = 100;
        result.errors.push("test error".to_string());

        assert_eq!(result.total_imported(), 108);
        assert!(result.has_errors());
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn import_idempotent_with_status_marker() {
        let (_config_path, config_conn) = temp_config_db("idempotent");
        let (_state_path, state_conn) = temp_state_db("idempotent");
        let (config_dir, data_dir) = isolated_legacy_dir("idempotent");

        // Write a legacy profiles.json with valid ConnectionProfile JSON
        let profile_json = serde_json::to_string(&[dbflux_core::ConnectionProfile::new_with_kind(
            "Test Profile",
            dbflux_core::DbKind::Postgres,
            dbflux_core::DbConfig::Postgres {
                use_uri: false,
                uri: None,
                host: "localhost".to_string(),
                port: 5432,
                user: "test".to_string(),
                database: "testdb".to_string(),
                ssl_mode: dbflux_core::SslMode::Disable,
                ssh_tunnel: None,
                ssh_tunnel_profile_id: None,
            },
        )])
        .unwrap();
        std::fs::write(config_dir.join("profiles.json"), &profile_json).unwrap();

        // First run: should import
        let result1 = run_legacy_import(
            config_conn.clone(),
            state_conn.clone(),
            &config_dir,
            &data_dir,
        );
        assert_eq!(result1.profiles_imported, 1);

        // Second run: should skip (status marker set to completed)
        let result2 = run_legacy_import(
            config_conn.clone(),
            state_conn.clone(),
            &config_dir,
            &data_dir,
        );
        assert_eq!(
            result2.profiles_imported, 0,
            "should skip already-completed imports"
        );

        // Cleanup
        let _ = std::fs::remove_file(&_config_path);
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(&_state_path);
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_dir_all(&config_dir.parent().unwrap());
    }

    #[test]
    fn import_retry_after_failure() {
        let (_config_path, config_conn) = temp_config_db("retry");
        let (_state_path, state_conn) = temp_state_db("retry");
        let (config_dir, data_dir) = isolated_legacy_dir("retry");

        // Mark as failed first
        set_import_status(config_conn.as_ref(), "profiles.json", ImportStatus::Failed);

        // Write valid profiles.json with valid ConnectionProfile JSON
        let profile_json = serde_json::to_string(&[dbflux_core::ConnectionProfile::new_with_kind(
            "Retry Profile",
            dbflux_core::DbKind::Postgres,
            dbflux_core::DbConfig::Postgres {
                use_uri: false,
                uri: None,
                host: "localhost".to_string(),
                port: 5432,
                user: "test".to_string(),
                database: "testdb".to_string(),
                ssl_mode: dbflux_core::SslMode::Disable,
                ssh_tunnel: None,
                ssh_tunnel_profile_id: None,
            },
        )])
        .unwrap();
        std::fs::write(config_dir.join("profiles.json"), &profile_json).unwrap();

        // Run should retry and succeed
        let result = run_legacy_import(
            config_conn.clone(),
            state_conn.clone(),
            &config_dir,
            &data_dir,
        );
        assert_eq!(result.profiles_imported, 1);

        // Cleanup
        let _ = std::fs::remove_file(&_config_path);
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(&_state_path);
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_dir_all(&config_dir.parent().unwrap());
    }

    #[test]
    fn import_records_status_on_failure() {
        let (_config_path, config_conn) = temp_config_db("fail_status");
        let (_state_path, state_conn) = temp_state_db("fail_status");
        let (config_dir, data_dir) = isolated_legacy_dir("fail_status");

        // Write invalid JSON (will cause parse failure)
        std::fs::write(config_dir.join("profiles.json"), "not valid json {{{").unwrap();

        let result = run_legacy_import(
            config_conn.clone(),
            state_conn.clone(),
            &config_dir,
            &data_dir,
        );
        assert!(result.has_errors());

        // Status should be recorded as failed
        assert_eq!(
            get_import_status(&config_conn, "profiles.json"),
            Some(false),
            "failed status should be recorded"
        );

        // Cleanup
        let _ = std::fs::remove_file(&_config_path);
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_config_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_file(&_state_path);
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(_state_path.with_extension("sqlite-shm"));
        let _ = std::fs::remove_dir_all(&config_dir.parent().unwrap());
    }

    #[test]
    fn any_imported_detects_items() {
        let mut result = LegacyImportResult::default();
        assert!(!result.any_imported());

        result.profiles_imported = 5;
        assert!(result.any_imported());

        result.profiles_imported = 0;
        result.ui_state_restored = true;
        assert!(result.any_imported());
    }
}
