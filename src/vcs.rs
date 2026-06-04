use std::collections::{HashMap, HashSet};
use std::process::Command;
use anyhow::{Context, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct VcsData {
    pub file_churn: HashMap<String, usize>,
    pub file_authors: HashMap<String, HashSet<String>>,
    pub file_last_update: HashMap<String, u64>,
    pub file_scatter: HashMap<String, f64>,
    pub file_agent_ratio: HashMap<String, f64>,
    pub couplings: Vec<Coupling>,
    pub now: u64,
}

#[derive(Debug, Clone)]
pub struct Coupling {
    pub file1: String,
    pub file2: String,
    pub degree: f64, // 0.0 to 1.0 (percentage of co-changes)
    pub revisions: usize,
}

impl VcsData {
    pub fn calculate(since: &str, path: &std::path::Path) -> Result<Self> {
        let output = Command::new("git")
            .arg("log")
            .arg(format!("--since={}", since))
            .arg("--name-only")
            .arg("--format=%x00COMMIT%x00%H%x00%ct%x00%an%x00%ae%x00%B%x00")
            .current_dir(path)
            .output()
            .context("Failed to execute git log")?;

        if !output.status.success() {
            anyhow::bail!("Git command failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        let mut file_churn = HashMap::new();
        let mut file_authors: HashMap<String, HashSet<String>> = HashMap::new();
        let mut file_last_update: HashMap<String, u64> = HashMap::new();
        let mut file_agent_commits: HashMap<String, usize> = HashMap::new();
        
        let mut commit_files: HashMap<String, Vec<String>> = HashMap::new();

        let records: Vec<&str> = stdout.split("\x00COMMIT\x00").collect();

        for record in records {
            if record.is_empty() {
                continue;
            }

            let parts: Vec<&str> = record.split('\x00').collect();
            if parts.len() >= 6 {
                let hash = parts[0].to_string();
                let time = parts[1].parse::<u64>().unwrap_or(0);
                let author = parts[2].to_string();
                let email = parts[3].to_string();
                let body = parts[4].to_string();
                let files_part = parts[5];

                let is_agent = is_bot(&author, &email, &body);

                let files: Vec<&str> = files_part.lines().filter(|l| !l.trim().is_empty()).collect();
                
                for file in files {
                    let file = file.to_string();
                    *file_churn.entry(file.clone()).or_insert(0) += 1;

                    if is_agent {
                        *file_agent_commits.entry(file.clone()).or_insert(0) += 1;
                    }

                    file_authors
                        .entry(file.clone())
                        .or_default()
                        .insert(author.clone());

                    let entry = file_last_update.entry(file.clone()).or_insert(time);
                    if time > *entry {
                        *entry = time;
                    }

                    commit_files
                        .entry(hash.clone())
                        .or_default()
                        .push(file);
                }
            }
        }

        // Calculate Agent Ratio
        let mut file_agent_ratio = HashMap::new();
        for (file, total) in &file_churn {
            let agents = *file_agent_commits.get(file).unwrap_or(&0);
            file_agent_ratio.insert(file.clone(), agents as f64 / *total as f64);
        }

        // Calculate Change Coupling and Scatter
        let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();
        let mut file_commit_sizes: HashMap<String, Vec<usize>> = HashMap::new();

        for (_, files) in &commit_files {
            let commit_size = files.len();
            for file in files {
                file_commit_sizes.entry(file.clone()).or_default().push(commit_size);
            }

            // Ignore massive commits (e.g. formatting, massive refactor) as they distort coupling
            if commit_size > 30 || commit_size < 2 {
                continue;
            }
            
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let mut pair = (files[i].clone(), files[j].clone());
                    if pair.0 > pair.1 {
                        pair = (pair.1, pair.0);
                    }
                    *pair_counts.entry(pair).or_insert(0) += 1;
                }
            }
        }

        let mut file_scatter = HashMap::new();
        for (file, sizes) in file_commit_sizes {
            if sizes.is_empty() { continue; }
            let total_files_touched: usize = sizes.iter().sum();
            let avg_commit_size = total_files_touched as f64 / sizes.len() as f64;
            // Scatter roughly maps to how distributed the file's changes are. 
            // We use log2 of avg commit size as a simple scatter penalty.
            let scatter = avg_commit_size.max(1.0).log2();
            file_scatter.insert(file, scatter);
        }

        let mut couplings = Vec::new();
        for ((file1, file2), count) in pair_counts {
            if count < 5 { // Minimum co-change threshold to filter noise
                continue;
            }
            
            let churn1 = *file_churn.get(&file1).unwrap_or(&0);
            let churn2 = *file_churn.get(&file2).unwrap_or(&0);
            
            // Jaccard similarity / Coupling degree
            // degree = co_changes / (churn1 + churn2 - co_changes)
            // Or simpler: percentage of commits of the least changed file
            let min_churn = std::cmp::min(churn1, churn2);
            if min_churn == 0 { continue; }
            let degree = count as f64 / min_churn as f64;
            
            if degree > 0.4 { // At least 40% coupled
                couplings.push(Coupling {
                    file1,
                    file2,
                    degree,
                    revisions: count,
                });
            }
        }
        
        couplings.sort_by(|a, b| b.degree.total_cmp(&a.degree));

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        Ok(VcsData { file_churn, file_authors, file_last_update, file_scatter, file_agent_ratio, couplings, now })
    }

    pub fn get_churn(&self, file_path: &str) -> usize {
        *self.file_churn.get(file_path).unwrap_or(&0)
    }

    pub fn get_author_count(&self, file_path: &str) -> usize {
        self.file_authors.get(file_path).map_or(0, |s| s.len())
    }
    
    pub fn get_scatter(&self, file_path: &str) -> f64 {
        *self.file_scatter.get(file_path).unwrap_or(&0.0)
    }

    pub fn get_agent_ratio(&self, file_path: &str) -> f64 {
        *self.file_agent_ratio.get(file_path).unwrap_or(&0.0)
    }
    
    pub fn get_age_months(&self, file_path: &str) -> f64 {
        let last_update = self.file_last_update.get(file_path).unwrap_or(&self.now);
        
        if *last_update == 0 || *last_update > self.now {
            return 0.0;
        }
        
        let diff_secs = self.now - *last_update;
        let months = diff_secs as f64 / (30.44 * 24.0 * 60.0 * 60.0);
        months
    }

    pub fn resolve_historical_path(&self, path: &std::path::Path, since: &str) -> Option<String> {
        let output = Command::new("git")
            .arg("log")
            .arg("--follow")
            .arg("--name-only")
            .arg("--format=")
            .arg(format!("--since={}", since))
            .arg("--")
            .arg(path)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().last().map(|s| s.to_string())
    }

    pub fn get_scope_churn(&self, path: &str, scope_name: &str, since: &str, base_path: &std::path::Path) -> usize {
        // Skip Global scope as it's just the file churn which we already have
        if scope_name == "Global" {
            return *self.file_churn.get(path).unwrap_or(&0);
        }

        let output = Command::new("git")
            .arg("log")
            .arg("-L")
            .arg(format!(":{}:{}", scope_name, path))
            .arg(format!("--since={}", since))
            .arg("--format=%H")
            .arg("--no-patch")
            .current_dir(base_path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.lines().filter(|l| !l.trim().is_empty()).count();
            }
        }
        0
    }
}

// Emails used by known AI coding assistants as Co-Authored-By identities.
const KNOWN_AI_EMAILS: &[&str] = &[
    "noreply@anthropic.com",   // Claude
    "copilot@github.com",      // GitHub Copilot
    "codeium@codeium.com",     // Codeium
];

fn is_bot_email(email: &str) -> bool {
    // GitHub bot noreply pattern: <id+name[bot]@users.noreply.github.com>
    if email.ends_with("@users.noreply.github.com") && email.contains("[bot]") {
        return true;
    }
    KNOWN_AI_EMAILS.iter().any(|&ai| email == ai)
}

fn is_bot(author: &str, email: &str, body: &str) -> bool {
    if author.to_lowercase().ends_with("[bot]") {
        return true;
    }

    if is_bot_email(&email.to_lowercase()) {
        return true;
    }

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("Generated-By:") {
            return true;
        }
        if line.starts_with("Co-Authored-By:") {
            // Extract email from "Co-Authored-By: Name <email>"
            if let (Some(lt), Some(gt)) = (line.rfind('<'), line.rfind('>')) {
                if lt < gt {
                    let coauthor_email = line[lt + 1..gt].to_lowercase();
                    if is_bot_email(&coauthor_email) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_bot() {
        // Author name ends with [bot]
        assert!(is_bot("dependabot[bot]", "123+dependabot[bot]@users.noreply.github.com", ""));
        // GitHub bot via noreply email alone
        assert!(is_bot("Dependabot", "49699333+dependabot[bot]@users.noreply.github.com", ""));
        // Generated-By trailer
        assert!(is_bot("some-author", "email@example.com", "Generated-By: AI"));
        // Co-Authored-By with real GitHub bot email format (email ends with github.com, not [bot]>)
        assert!(is_bot("some-author", "email@example.com",
            "Co-Authored-By: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>"));
        // Co-Authored-By with known AI service email
        assert!(is_bot("some-author", "email@example.com",
            "Co-Authored-By: Claude <noreply@anthropic.com>"));
        // Human name must not match
        assert!(!is_bot("Claude Martin", "claude@example.com", "Fixed a bug"));
        // Human Co-Authored-By must not match
        assert!(!is_bot("bot-helper", "helper@example.com",
            "Co-Authored-By: Alice <alice@example.com>"));
    }
}
