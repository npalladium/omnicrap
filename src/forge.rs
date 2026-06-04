#![cfg(feature = "wip")]
use anyhow::Result;
use async_trait::async_trait;

/// A trait for interacting with code hosting platforms like GitHub or GitLab.
#[async_trait]
pub trait RepositoryForge: Send + Sync {
    fn name(&self) -> &str;
    
    /// Get metadata for a specific file, such as review frequency or comment density.
    async fn get_file_metadata(&self, path: &str) -> Result<ForgeFileMetadata>;
    
    /// Get high-level repository health metrics.
    async fn get_repo_health(&self) -> Result<RepoHealth>;
}

pub struct ForgeFileMetadata {
    pub total_comments: usize,
    pub review_revisions: usize,
    pub last_reviewed_at: u64,
}

pub struct RepoHealth {
    pub open_prs: usize,
    pub avg_pr_lifetime_days: f64,
}

/// A trait for interacting with project management tools like Jira or Linear.
#[async_trait]
pub trait WorkItemTracker: Send + Sync {
    fn name(&self) -> &str;

    /// Get work items (bugs, features) associated with a specific file based on commit links.
    async fn get_items_for_file(&self, path: &str) -> Result<Vec<WorkItem>>;

    /// Get the "Hotspot Score" based on bug density.
    async fn get_bug_density(&self, path: &str) -> Result<f64>;
}

pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub kind: WorkItemKind,
    pub status: String,
}

pub enum WorkItemKind {
    Bug,
    Feature,
    Task,
    Refactor,
}
