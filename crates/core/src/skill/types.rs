use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceKind {
    Project,
    LocalPath,
    Remote,
}

impl SkillSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::LocalPath => "local_path",
            Self::Remote => "remote",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "project" => Some(Self::Project),
            "local_path" => Some(Self::LocalPath),
            "remote" => Some(Self::Remote),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub location: String,
    pub content: String,
    pub source_kind: SkillSourceKind,
    pub source_url: Option<String>,
    pub content_hash: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct DiscoveredSkill {
    pub info: SkillInfo,
    pub base_dir: Option<PathBuf>,
}
