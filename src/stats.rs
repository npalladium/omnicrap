use std::path::Path;
use anyhow::Result;
use crate::languages::LanguageDatabase;
use serde::Serialize;

#[derive(Debug, Default, Serialize, Clone)]
pub struct FileStats {
    pub lines: usize,
    pub code: usize,
    pub comments: usize,
    pub blanks: usize,
}

pub struct StatsEngine;

impl StatsEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, path: &Path, content: &str) -> Result<FileStats> {
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let content = content.as_bytes();
        
        let lang = match LanguageDatabase::get().get_by_extension(extension) {
            Some(l) => l,
            None => {
                return Ok(self.simple_count(content));
            }
        };

        let mut stats = FileStats::default();
        let mut i = 0;
        let len = content.len();
        
        let mut in_string: Option<String> = None;
        let mut comment_stack: Vec<String> = Vec::new(); // For multi-line
        let mut is_line_code = false;
        let mut is_line_comment = false;

        while i < len {
            let b = content[i];

            // Handle Newline
            if b == b'\n' {
                stats.lines += 1;
                if is_line_code {
                    stats.code += 1;
                } else if is_line_comment || !comment_stack.is_empty() {
                    stats.comments += 1;
                } else {
                    stats.blanks += 1;
                }
                is_line_code = false;
                is_line_comment = false;
                i += 1;
                continue;
            }

            if b.is_ascii_whitespace() {
                i += 1;
                continue;
            }

            // String state
            if let Some(quote_end) = in_string.clone() {
                if self.match_bytes(&content, i, &quote_end) {
                    // Check for escape
                    if i > 0 && content[i-1] == b'\\' {
                        let mut backslashes = 0;
                        let mut j = (i as isize) - 1;
                        while j >= 0 && content[j as usize] == b'\\' {
                            backslashes += 1;
                            j -= 1;
                        }
                        if backslashes % 2 == 1 {
                            is_line_code = true;
                            i += quote_end.len();
                            continue;
                        }
                    }
                    in_string = None;
                    is_line_code = true;
                    i += quote_end.len();
                } else {
                    is_line_code = true;
                    i += 1;
                }
                continue;
            }

            // Multi-line comment state
            if !comment_stack.is_empty() {
                let current_end = comment_stack.last().unwrap().clone();
                if self.match_bytes(&content, i, &current_end) {
                    comment_stack.pop();
                    is_line_comment = true;
                    i += current_end.len();
                } else {
                    // SCC doesn't seem to have a 'nested' flag in JSON, 
                    // but we can optionally check for nested starts if we want to be fancy.
                    // For now, let's keep it simple as per most languages.
                    i += 1;
                }
                continue;
            }

            // Check for new string
            let mut matched_string = false;
            for q in &lang.quotes {
                if self.match_bytes(&content, i, &q.start) {
                    in_string = Some(q.end.clone());
                    is_line_code = true;
                    i += q.start.len();
                    matched_string = true;
                    break;
                }
            }
            if matched_string { continue; }

            // Check for single line comment
            let mut matched_line_comment = false;
            for start in &lang.line_comment {
                if self.match_bytes(&content, i, start) {
                    is_line_comment = true;
                    while i < len && content[i] != b'\n' {
                        i += 1;
                    }
                    matched_line_comment = true;
                    break;
                }
            }
            if matched_line_comment { continue; }

            // Check for multi-line comment start
            let mut matched_multi_start = false;
            for pair in &lang.multi_line {
                if pair.len() == 2 && self.match_bytes(&content, i, &pair[0]) {
                    comment_stack.push(pair[1].clone());
                    is_line_comment = true;
                    i += pair[0].len();
                    matched_multi_start = true;
                    break;
                }
            }
            if matched_multi_start { continue; }

            is_line_code = true;
            i += 1;
        }

        if i > 0 && content[len-1] != b'\n' {
            stats.lines += 1;
            if is_line_code {
                stats.code += 1;
            } else if is_line_comment || !comment_stack.is_empty() {
                stats.comments += 1;
            } else {
                stats.blanks += 1;
            }
        }

        Ok(stats)
    }

    fn match_bytes(&self, content: &[u8], i: usize, target: &str) -> bool {
        let target_bytes = target.as_bytes();
        if i + target_bytes.len() > content.len() {
            return false;
        }
        &content[i..i+target_bytes.len()] == target_bytes
    }

    fn simple_count(&self, content: &[u8]) -> FileStats {
        let mut stats = FileStats::default();
        for line in content.split(|&b| b == b'\n') {
            stats.lines += 1;
            if line.iter().all(|b| b.is_ascii_whitespace()) {
                stats.blanks += 1;
            } else {
                stats.code += 1;
            }
        }
        stats
    }
}
