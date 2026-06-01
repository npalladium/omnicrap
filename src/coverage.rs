use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::Result;

pub struct CoverageData {
    // Maps file path to a map of line number -> hit count
    pub files: HashMap<String, HashMap<usize, usize>>,
}

impl CoverageData {
    pub fn parse_lcov(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut files = HashMap::new();
        let mut current_file = String::new();
        let mut current_coverage = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("SF:") {
                current_file = line[3..].to_string();
                current_coverage = HashMap::new();
            } else if line.starts_with("DA:") {
                let parts: Vec<&str> = line[3..].split(',').collect();
                if parts.len() >= 2 {
                    let line_num: usize = parts[0].parse()?;
                    let hit_count: usize = parts[1].parse()?;
                    current_coverage.insert(line_num, hit_count);
                }
            } else if line == "end_of_record" {
                files.insert(current_file.clone(), current_coverage.clone());
            }
        }

        Ok(CoverageData { files })
    }

    pub fn get_function_coverage(&self, file_path: &str, start_line: usize, end_line: usize) -> f64 {
        let file_coverage = match self.files.get(file_path) {
            Some(c) => c,
            None => return 0.0,
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
