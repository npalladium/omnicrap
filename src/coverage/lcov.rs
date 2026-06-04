use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::Result;
use crate::coverage::{CoverageData, CoverageParser};

pub struct LcovParser;

impl CoverageParser for LcovParser {
    fn name(&self) -> &str {
        "lcov"
    }

    fn can_parse(&self, path: &Path) -> bool {
        if path.to_string_lossy().ends_with("lcov.info") || path.extension().map_or(false, |ext| ext == "info") {
            return true;
        }

        if let Ok(file) = File::open(path) {
            let mut reader = BufReader::new(file);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).is_ok() {
                let trimmed = first_line.trim_start_matches('\u{feff}');
                return trimmed.starts_with("TN:") || trimmed.starts_with("SF:");
            }
        }
        false
    }

    fn parse(&self, path: &Path) -> Result<CoverageData> {
        // ... (existing parse logic)
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        self.parse_reader(reader)
    }
}

impl LcovParser {
    fn parse_reader<R: BufRead>(&self, reader: R) -> Result<CoverageData> {
        let mut files = HashMap::new();
        let mut current_file = String::new();
        let mut current_coverage = HashMap::new();

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("SF:") {
                current_file = line[3..].trim().to_string();
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_lcov_parsing_with_crlf() {
        let lcov_data = "TN:\r\nSF:src/main.rs\r\nDA:1,1\r\nDA:2,0\r\nend_of_record\r\n";
        let parser = LcovParser;
        let data = parser.parse_reader(Cursor::new(lcov_data)).unwrap();
        
        let file_cov = data.files.get("src/main.rs").unwrap();
        assert_eq!(*file_cov.get(&1).unwrap(), 1);
        assert_eq!(*file_cov.get(&2).unwrap(), 0);
    }
}
