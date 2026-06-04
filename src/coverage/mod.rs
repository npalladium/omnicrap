pub mod lcov;
pub mod cobertura;

use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;

pub trait CoverageParser {
    fn name(&self) -> &str;
    fn can_parse(&self, path: &Path) -> bool;
    fn parse(&self, path: &Path) -> Result<CoverageData>;
}

pub struct CoverageData {
    // Maps file path to a map of line number -> hit count
    pub files: HashMap<String, HashMap<usize, usize>>,
}

impl CoverageData {
    pub fn get_function_coverage(&self, file_path: &str, start_line: usize, end_line: usize) -> f64 {
        let normalized_target = file_path.replace('\\', "/");
        
        let file_coverage = if let Some(c) = self.files.get(&normalized_target) {
            c
        } else {
            // Best-effort suffix match (e.g., "src/main.rs" matches "/abs/path/src/main.rs")
            let mut found = None;
            for (path, cov) in &self.files {
                let normalized_path = path.replace('\\', "/");
                if normalized_path.ends_with(&normalized_target) || normalized_target.ends_with(&normalized_path) {
                    found = Some(cov);
                    break;
                }
            }
            match found {
                Some(c) => c,
                None => return 0.0,
            }
        };

        let mut instrumented = 0;
        let mut hit = 0;

        for line in start_line..=end_line {
            if let Some(&count) = file_coverage.get(&line) {
                instrumented += 1;
                if count > 0 {
                    hit += 1;
                }
            }
        }

        if instrumented == 0 {
            0.0
        } else {
            hit as f64 / instrumented as f64
        }
    }
}
