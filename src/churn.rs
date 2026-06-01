use std::collections::HashMap;
use std::process::Command;
use anyhow::{Context, Result};

pub struct ChurnData {
    pub file_churn: HashMap<String, usize>,
}

impl ChurnData {
    pub fn calculate(since: &str, path: &std::path::Path) -> Result<Self> {
        let output = Command::new("git")
            .arg("log")
            .arg(format!("--since={}", since))
            .arg("--name-only")
            .arg("--format=")
            .current_dir(path)
            .output()
            .context("Failed to execute git log")?;

        if !output.status.success() {
            anyhow::bail!("Git command failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut file_churn = HashMap::new();

        for line in stdout.lines() {
            let line = line.trim();
            if !line.is_empty() {
                *file_churn.entry(line.to_string()).or_insert(0) += 1;
            }
        }

        Ok(ChurnData { file_churn })
    }

    pub fn get_churn(&self, file_path: &str) -> usize {
        *self.file_churn.get(file_path).unwrap_or(&0)
    }
}
