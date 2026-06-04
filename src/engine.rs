use std::path::Path;
use anyhow::Result;
use serde::Serialize;

use crate::clone_engine::CloneMatch;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    Function,
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TestKind {
    Unit,        // Pure, module-level
    Integration, // Pure, API-level
    E2E,         // Impure, touches external world (Disk, Network, DB)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub complexity: f64,
    pub halstead: f64,
    pub redundancy: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScopeInfo {
    pub name: String,
    pub kind: ScopeKind,
    pub test_kind: Option<TestKind>,
    pub mock_count: usize,
    pub clone_ratio: f64,
    pub clone_matches: Vec<CloneMatch>,
    pub start_line: usize,
    pub end_line: usize,
    pub metrics: Metrics,
}

pub trait LanguageEngine: Send + Sync {
    fn name(&self) -> &str;
    fn is_supported(&self, extension: &str) -> bool;
    fn analyze(&self, path: &Path, content: &str) -> Result<Vec<ScopeInfo>>;
    fn register_clones(&self, _path: &Path, _content: &str) -> Result<()> { Ok(()) }
}
