use std::path::Path;
use anyhow::Result;
use crate::engine::{LanguageEngine, ScopeInfo, ScopeKind};
use crate::languages::LanguageDatabase;
use regex::Regex;
use std::collections::HashMap;

pub struct RegexEngine;

impl RegexEngine {
    pub fn new() -> Self {
        Self
    }

    fn calculate_complexity_for_slice(&self, slice: &str, complexity_checks: &[String]) -> usize {
        let mut complexity = 1;
        for check in complexity_checks {
            let escaped = regex::escape(check);
            if let Ok(re) = Regex::new(&escaped) {
                complexity += re.find_iter(slice).count();
            }
        }
        complexity
    }

    fn find_scopes(&self, content: &str, extension: &str) -> Vec<(usize, usize, String)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut scopes = Vec::new();

        // Heuristic for brace-based languages
        if matches!(extension, "c" | "cpp" | "java" | "js" | "ts" | "cs" | "go" | "php" | "rs" | "swift" | "kt" | "scala") {
            let mut stack = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                if trimmed.contains('{') && (trimmed.contains("fn ") || trimmed.contains("func") || trimmed.contains("function") || trimmed.contains("class ") || trimmed.contains("public ") || trimmed.contains("private ")) {
                    stack.push(i);
                }
                if trimmed.contains('}') && !stack.is_empty() {
                    let start = stack.pop().unwrap();
                    if i - start > 2 { // Only care about blocks with some substance
                        scopes.push((start + 1, i + 1, format!("block_at_line_{}", start + 1)));
                    }
                }
            }
        } 
        // Heuristic for indent-based languages (Python)
        else if matches!(extension, "py" | "rb" | "sh") {
            let mut current_scope_start = None;
            let mut current_indent = 0;

            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
                
                let indent = line.len() - trimmed.len();
                if trimmed.starts_with("def ") || trimmed.starts_with("class ") || trimmed.starts_with("if ") || trimmed.starts_with("for ") || trimmed.starts_with("while ") {
                    if current_scope_start.is_none() {
                        current_scope_start = Some(i);
                        current_indent = indent;
                    }
                } else if let Some(start) = current_scope_start {
                    if indent <= current_indent && !trimmed.is_empty() {
                        if i - start > 2 {
                            scopes.push((start + 1, i, format!("block_at_line_{}", start + 1)));
                        }
                        current_scope_start = None;
                    }
                }
            }
            if let Some(start) = current_scope_start {
                scopes.push((start + 1, lines.len(), format!("block_at_line_{}", start + 1)));
            }
        }

        scopes
    }
}

impl LanguageEngine for RegexEngine {
    fn name(&self) -> &str {
        "regex-fallback"
    }

    fn is_supported(&self, extension: &str) -> bool {
        LanguageDatabase::get().get_by_extension(extension).is_some()
    }

    fn analyze(&self, _path: &Path, content: &str) -> Result<Vec<ScopeInfo>> {
        let extension = _path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let lang = LanguageDatabase::get().get_by_extension(extension).unwrap();
        
        let mut scopes = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // 1. Add Global Scope
        let mut global_metrics = HashMap::new();
        let total_complexity = self.calculate_complexity_for_slice(content, &lang.complexitychecks);
        global_metrics.insert("complexity".to_string(), total_complexity as f64);
        
        scopes.push(ScopeInfo {
            name: "Global".to_string(),
            kind: ScopeKind::Module,
            test_kind: None, 
            mock_count: 0,
            clone_ratio: 0.0,
            clone_matches: Vec::new(),
            start_line: 1,
            end_line: lines.len().max(1),
            metrics: global_metrics,
        });

        // 2. Add detected sub-scopes
        let found = self.find_scopes(content, extension);
        for (start, end, name) in found {
            let slice = lines[start-1..end].join("\n");
            let complexity = self.calculate_complexity_for_slice(&slice, &lang.complexitychecks);
            
            let mut metrics = HashMap::new();
            metrics.insert("complexity".to_string(), complexity as f64);

            scopes.push(ScopeInfo {
                name,
                kind: ScopeKind::Function,
                test_kind: None,
                mock_count: 0,
                clone_ratio: 0.0,
                clone_matches: Vec::new(),
                start_line: start,
                end_line: end,
                metrics,
            });
        }

        Ok(scopes)
    }
}
