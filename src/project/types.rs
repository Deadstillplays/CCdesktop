use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub folder_path: String,
    pub instructions: String, // Project custom instructions / CLAUDE.md
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFileEntry {
    pub relative_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub updated_at: DateTime<Utc>,
}
