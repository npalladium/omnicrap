#[cfg(test)]
mod tests {
    use omni_crap::coverage::CoverageData;
    use std::collections::HashMap;

    #[test]
    fn test_coverage_path_matching() {
        let mut files = HashMap::new();
        let mut line_cov = HashMap::new();
        line_cov.insert(1, 1);
        
        // Report has absolute path
        files.insert("/abs/path/src/main.rs".to_string(), line_cov.clone());
        files.insert("C:\\Windows\\Path\\src\\lib.rs".to_string(), line_cov.clone());
        
        let data = CoverageData { files };
        
        // Match relative path with forward slashes
        assert_eq!(data.get_function_coverage("src/main.rs", 1, 1), 1.0);
        
        // Match relative path with mixed slashes
        assert_eq!(data.get_function_coverage("src\\main.rs", 1, 1), 1.0);
        
        // Match lib.rs with different separators
        assert_eq!(data.get_function_coverage("src/lib.rs", 1, 1), 1.0);
        
        // Non-existent file
        assert_eq!(data.get_function_coverage("src/other.rs", 1, 1), 0.0);
    }
}
