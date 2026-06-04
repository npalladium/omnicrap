use std::path::Path;
use anyhow::Result;
use crate::engine::{LanguageEngine, ScopeInfo, ScopeKind};
use crate::languages::LanguageDatabase;
use regex::Regex;
use std::collections::HashMap;

/// Extract `(ScopeKind, name)` from a `def`/`class` header line (indent-based languages).
/// Returns `None` for control-flow lines (`if`, `for`, `while`, etc.).
fn extract_indent_scope(trimmed: &str) -> Option<(ScopeKind, String)> {
    if let Some(rest) = trimmed.strip_prefix("def ") {
        let name = rest.split(|c| c == '(' || c == ':').next()?.trim().to_string();
        if !name.is_empty() { return Some((ScopeKind::Function, name)); }
    }
    if let Some(rest) = trimmed.strip_prefix("class ") {
        let name = rest.split(|c| c == '(' || c == ':').next()?.trim().to_string();
        if !name.is_empty() { return Some((ScopeKind::Class, name)); }
    }
    None
}

/// Extract `(ScopeKind, name)` from a line that opens a brace-delimited scope.
/// Recognises common function/class keywords across languages.
fn extract_brace_scope(line: &str) -> Option<(ScopeKind, String)> {
    let trimmed = line.trim();
    // Keyword patterns that precede a name: function, func, fn, def, sub, proc
    // Class patterns: class, struct, interface, enum, trait, impl
    let class_keywords = ["class ", "struct ", "interface ", "trait ", "enum "];
    let func_keywords  = ["function ", "func ", "fn ", "def ", "sub ", "proc "];

    for kw in &class_keywords {
        if let Some(rest) = trimmed.find(kw).map(|p| &trimmed[p + kw.len()..]) {
            let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_').next()?.to_string();
            if !name.is_empty() { return Some((ScopeKind::Class, name)); }
        }
    }
    for kw in &func_keywords {
        if let Some(rest) = trimmed.find(kw).map(|p| &trimmed[p + kw.len()..]) {
            let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_').next()?.to_string();
            if !name.is_empty() { return Some((ScopeKind::Function, name)); }
        }
    }
    None
}

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

    fn find_scopes(&self, content: &str, extension: &str) -> Vec<(usize, usize, String, ScopeKind)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut scopes = Vec::new();

        if matches!(extension, "py" | "rb" | "sh") {
            // Indent-based: only `def`/`class` open a scope — not control flow.
            // Stack: (start_line_1idx, indent_depth, name, kind)
            let mut stack: Vec<(usize, usize, String, ScopeKind)> = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
                let indent = line.len() - trimmed.len();

                // Pop any scopes that dedented back to or past this level.
                loop {
                    match stack.last() {
                        Some((_start_line, start_indent, _, _)) if indent <= *start_indent => {
                            let (start_line, _, name, kind) = stack.pop().unwrap();
                            if i + 1 > start_line {
                                scopes.push((start_line, i, name, kind));
                            }
                        }
                        _ => break,
                    }
                }

                if let Some((kind, name)) = extract_indent_scope(trimmed) {
                    stack.push((i + 1, indent, name, kind));
                }
            }
            // Flush still-open scopes at EOF.
            for (start_line, _, name, kind) in stack {
                scopes.push((start_line, lines.len(), name, kind));
            }
        } else {
            // Brace-based: track `{` / `}` depth; detect function/class headers.
            // Stack: (start_line_1idx, name, kind, open_depth)
            let mut stack: Vec<(usize, String, ScopeKind, usize)> = Vec::new();
            let mut depth = 0usize;

            for (i, line) in lines.iter().enumerate() {
                let opens  = line.chars().filter(|&c| c == '{').count();
                let closes = line.chars().filter(|&c| c == '}').count();

                // Detect header on lines that open a brace.
                if opens > 0 {
                    if let Some((kind, name)) = extract_brace_scope(line) {
                        stack.push((i + 1, name, kind, depth));
                    }
                }

                depth = depth.saturating_add(opens).saturating_sub(closes);

                // Close scopes whose opening depth has been reached again.
                loop {
                    match stack.last() {
                        Some((_start_line, _, _, open_depth)) if depth <= *open_depth => {
                            let (start_line, name, kind, _) = stack.pop().unwrap();
                            if i + 1 > start_line {
                                scopes.push((start_line, i + 1, name, kind));
                            }
                        }
                        _ => break,
                    }
                }
            }
        }

        scopes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> RegexEngine { RegexEngine::new() }

    // ── indent-based (Python) ──────────────────────────────────────────────

    #[test]
    fn python_def_extracts_name() {
        let src = "def compute(x):\n    return x + 1\n";
        let scopes = engine().find_scopes(src, "py");
        assert!(scopes.iter().any(|s| s.2 == "compute"),
            "expected scope named 'compute', got {:?}", scopes.iter().map(|s| &s.2).collect::<Vec<_>>());
    }

    #[test]
    fn python_class_extracts_name() {
        let src = "class Parser:\n    def __init__(self):\n        pass\n";
        let scopes = engine().find_scopes(src, "py");
        assert!(scopes.iter().any(|s| s.2 == "Parser"),
            "expected scope named 'Parser', got {:?}", scopes.iter().map(|s| &s.2).collect::<Vec<_>>());
    }

    #[test]
    fn python_if_does_not_create_scope() {
        let src = "if True:\n    x = 1\n    y = 2\n    z = 3\n";
        let scopes = engine().find_scopes(src, "py");
        assert!(scopes.is_empty(), "if-block should not become a scope, got {:?}", scopes);
    }

    #[test]
    fn python_for_does_not_create_scope() {
        let src = "for i in range(10):\n    print(i)\n    print(i*2)\n    print(i*3)\n";
        let scopes = engine().find_scopes(src, "py");
        assert!(scopes.is_empty(), "for-loop should not become a scope, got {:?}", scopes);
    }

    // ── brace-based (generic / non-TS languages) ──────────────────────────

    #[test]
    fn brace_function_extracts_name() {
        let src = "function doWork(x) {\n    return x;\n}\n";
        let scopes = engine().find_scopes(src, "lua");
        // any brace language that regex engine handles
        assert!(scopes.iter().any(|s| s.2 == "doWork"),
            "expected scope named 'doWork', got {:?}", scopes.iter().map(|s| &s.2).collect::<Vec<_>>());
    }

    #[test]
    fn brace_scope_kind_is_function() {
        let src = "function doWork(x) {\n    return x;\n}\n";
        let scopes = engine().find_scopes(src, "lua");
        let found = scopes.iter().find(|s| s.2 == "doWork");
        assert!(found.is_some());
        assert_eq!(found.unwrap().3, ScopeKind::Function);
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
        for (start, end, name, kind) in found {
            let slice = lines[start-1..end.min(lines.len())].join("\n");
            let complexity = self.calculate_complexity_for_slice(&slice, &lang.complexitychecks);

            let mut metrics = HashMap::new();
            metrics.insert("complexity".to_string(), complexity as f64);

            scopes.push(ScopeInfo {
                name,
                kind,
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
