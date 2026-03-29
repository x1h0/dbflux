//! Flat DTOs for config and state repositories.
//!
//! These DTOs are used by the repository layer for serializing/deserializing
//! database records. They use flat, non-nested columnar structure.

use serde::{Deserialize, Serialize};

use uuid::Uuid;

/// DTO for service/RPC definitions (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDto {
    pub socket_id: String,
    pub enabled: bool,
    pub command: String,
    pub args_json: Option<String>,
    pub env_json: Option<String>,
    pub startup_timeout_ms: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl ServiceDto {
    /// Creates a new DTO.
    pub fn new(socket_id: String, command: String) -> Self {
        Self {
            socket_id,
            enabled: true,
            command,
            args_json: None,
            env_json: None,
            startup_timeout_ms: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// DTO for proxy profile storage (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProfileDto {
    pub profile_id: String,
    pub name: String,
    pub kind: String, // "http" | "socks5"
    pub host: String,
    pub port: i32,
    pub auth_kind: String, // "none" | "basic" | "ntlm"
    pub auth_username: Option<String>,
    pub auth_password_secret_ref: Option<String>,
    pub no_proxy: Option<String>,
    pub enabled: bool,
    pub save_secret: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl ProxyProfileDto {
    /// Creates a new DTO.
    pub fn new(profile_id: String, name: String, kind: String, host: String, port: i32) -> Self {
        Self {
            profile_id,
            name,
            kind,
            host,
            port,
            auth_kind: "none".to_string(),
            auth_username: None,
            auth_password_secret_ref: None,
            no_proxy: None,
            enabled: true,
            save_secret: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// DTO for SSH tunnel profile storage (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelProfileDto {
    pub profile_id: String,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub user: String,
    pub auth_method: String, // "password" | "key"
    pub key_path: Option<String>,
    pub passphrase_secret_ref: Option<String>,
    pub save_secret: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl SshTunnelProfileDto {
    /// Creates a new DTO.
    pub fn new(profile_id: String, name: String, host: String, port: i32, user: String) -> Self {
        Self {
            profile_id,
            name,
            host,
            port,
            user,
            auth_method: "password".to_string(),
            key_path: None,
            passphrase_secret_ref: None,
            save_secret: false,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

/// DTO for hook definition storage (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinitionDto {
    pub hook_id: String,
    pub name: String,
    pub kind: String, // "command" | "script" | "lua"
    pub command: Option<String>,
    pub args_json: Option<String>,
    pub script_language: Option<String>,    // "bash" | "python"
    pub script_source_type: Option<String>, // "inline" | "file"
    pub script_content: Option<String>,
    pub script_path: Option<String>,
    pub interpreter: Option<String>,
    pub cwd: Option<String>,
    pub env_json: Option<String>,
    pub inherit_env: bool,
    pub timeout_ms: Option<i64>,
    pub ready_signal: Option<String>,
    pub execution_mode: String, // "blocking" | "detached"
    pub on_failure: String,     // "warn" | "ignore" | "disconnect"
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl HookDefinitionDto {
    /// Creates a new DTO.
    pub fn new(hook_id: String, name: String, kind: String, execution_mode: String) -> Self {
        Self {
            hook_id,
            name,
            kind,
            command: None,
            args_json: None,
            script_language: None,
            script_source_type: None,
            script_content: None,
            script_path: None,
            interpreter: None,
            cwd: None,
            env_json: None,
            inherit_env: true,
            timeout_ms: None,
            ready_signal: None,
            execution_mode,
            on_failure: "warn".to_string(),
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// State DTOs
// ---------------------------------------------------------------------------

/// DTO for UI runtime state (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiStateDto {
    pub state_key: String,
    pub theme: Option<String>,
    pub sidebar_width: Option<i64>,
    pub sidebar_collapsed: Option<bool>,
    pub active_tabs_json: Option<String>,
    pub panel_sizes_json: Option<String>,
    pub updated_at: String,
}

impl UiStateDto {
    /// Creates a new DTO.
    pub fn new(state_key: String) -> Self {
        Self {
            state_key,
            theme: None,
            sidebar_width: None,
            sidebar_collapsed: None,
            active_tabs_json: None,
            panel_sizes_json: None,
            updated_at: String::new(),
        }
    }
}

/// DTO for query history entries (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryDto {
    pub entry_id: String,
    pub connection_profile_id: Option<String>,
    pub driver_id: Option<String>,
    pub database_name: Option<String>,
    pub query_text: String,
    pub query_kind: String,
    pub executed_at: String,
    pub duration_ms: Option<i64>,
    pub succeeded: bool,
    pub error_summary: Option<String>,
    pub row_count: Option<i64>,
    pub is_favorite: bool,
}

impl QueryHistoryDto {
    /// Creates a new DTO.
    pub fn new(
        query_text: String,
        connection_profile_id: Option<String>,
        driver_id: Option<String>,
        database_name: Option<String>,
        query_kind: String,
        duration_ms: Option<i64>,
        succeeded: bool,
        error_summary: Option<String>,
        row_count: Option<i64>,
    ) -> Self {
        Self {
            entry_id: Uuid::new_v4().to_string(),
            connection_profile_id,
            driver_id,
            database_name,
            query_text,
            query_kind,
            executed_at: String::new(),
            duration_ms,
            succeeded,
            error_summary,
            row_count,
            is_favorite: false,
        }
    }
}

/// DTO for recent items (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentItemDto {
    pub item_id: String,
    pub kind: String,
    pub profile_id: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub accessed_at: String,
}

impl RecentItemDto {
    /// Creates a DTO for a file item.
    pub fn file(id: Uuid, path: String, title: String) -> Self {
        Self {
            item_id: id.to_string(),
            kind: "file".to_string(),
            profile_id: None,
            path: Some(path),
            title: Some(title),
            accessed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Creates a DTO for a connection item.
    pub fn connection(id: Uuid, profile_id: Uuid, title: String) -> Self {
        Self {
            item_id: id.to_string(),
            kind: "connection".to_string(),
            profile_id: Some(profile_id.to_string()),
            path: None,
            title: Some(title),
            accessed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// DTO for saved query folders (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQueryFolderDto {
    pub folder_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub position: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// DTO for saved queries (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQueryDto {
    pub query_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub sql: String,
    pub is_favorite: bool,
    pub connection_id: Option<String>,
    pub created_at: String,
    pub last_used_at: String,
}

impl SavedQueryDto {
    /// Creates a new DTO.
    pub fn new(name: String, sql: String, connection_id: Option<Uuid>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            query_id: Uuid::new_v4().to_string(),
            folder_id: None,
            name,
            sql,
            is_favorite: false,
            connection_id: connection_id.map(|u| u.to_string()),
            created_at: now.clone(),
            last_used_at: now,
        }
    }
}

/// DTO for sessions (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDto {
    pub session_id: String,
    pub name: String,
    pub kind: String,
    pub workspace_path: Option<String>,
    pub active_index: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub last_opened_at: String,
    pub is_last_active: bool,
}

/// DTO for session tabs (flat columns).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTabDto {
    pub tab_id: String,
    pub session_id: String,
    pub tab_kind: String,
    pub language: String,
    pub title: String,
    pub position: i64,
    pub is_pinned: bool,
    pub exec_ctx_connection_id: Option<String>,
    pub exec_ctx_database: Option<String>,
    pub exec_ctx_schema: Option<String>,
    pub exec_ctx_container: Option<String>,
    pub scratch_content: Option<String>,
    pub file_path: Option<String>,
    pub shadow_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
