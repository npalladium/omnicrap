use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use anyhow::Result;
use serde::Deserialize;
use crate::coverage::{CoverageData, CoverageParser};

pub struct CoberturaParser;

#[derive(Debug, Deserialize)]
struct CoberturaCoverage {
    packages: CoberturaPackages,
}

#[derive(Debug, Deserialize)]
struct CoberturaPackages {
    #[serde(rename = "package", default)]
    packages: Vec<CoberturaPackage>,
}

#[derive(Debug, Deserialize)]
struct CoberturaPackage {
    classes: CoberturaClasses,
}

#[derive(Debug, Deserialize)]
struct CoberturaClasses {
    #[serde(rename = "class", default)]
    classes: Vec<CoberturaClass>,
}

#[derive(Debug, Deserialize)]
struct CoberturaClass {
    #[serde(rename = "@filename")]
    filename: String,
    lines: CoberturaLines,
}

#[derive(Debug, Deserialize)]
struct CoberturaLines {
    #[serde(rename = "line", default)]
    lines: Vec<CoberturaLine>,
}

#[derive(Debug, Deserialize)]
struct CoberturaLine {
    #[serde(rename = "@number")]
    number: usize,
    #[serde(rename = "@hits")]
    hits: usize,
}

impl CoverageParser for CoberturaParser {
    fn name(&self) -> &str {
        "cobertura"
    }

    fn can_parse(&self, path: &Path) -> bool {
        if let Ok(file) = File::open(path) {
            let mut reader = BufReader::new(file);
            let mut buf = [0u8; 512];
            use std::io::Read;
            if let Ok(n) = reader.read(&mut buf) {
                let content = String::from_utf8_lossy(&buf[..n]);
                if content.contains("<coverage") && content.contains("<packages") {
                    return true;
                }
            }
        }

        path.to_string_lossy().contains("cobertura")
    }

    fn parse(&self, path: &Path) -> Result<CoverageData> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        
        let report: CoberturaCoverage = quick_xml::de::from_reader(reader)?;
        
        let mut files = HashMap::new();
        
        for package in report.packages.packages {
            for class in package.classes.classes {
                let mut line_coverage = HashMap::new();
                for line in class.lines.lines {
                    line_coverage.insert(line.number, line.hits);
                }
                files.insert(class.filename, line_coverage);
            }
        }

        Ok(CoverageData { files })
    }
}
