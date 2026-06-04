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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_tmp(content: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn rejects_generic_xml() {
        let tmp = write_tmp(b"<?xml version=\"1.0\"?>\n<report><data/></report>\n");
        assert!(!CoberturaParser.can_parse(tmp.path()),
            "generic XML without <coverage/<packages should not match");
    }

    #[test]
    fn accepts_cobertura_xml() {
        let tmp = write_tmp(b"<?xml version=\"1.0\"?>\n<!DOCTYPE coverage>\n<coverage><packages><package/></packages></coverage>\n");
        assert!(CoberturaParser.can_parse(tmp.path()),
            "valid cobertura XML should match on content");
    }

    #[test]
    fn accepts_cobertura_by_path_fallback() {
        // A file that contains neither marker but has "cobertura" in the path name
        // still matches via path fallback.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cobertura.xml");
        std::fs::write(&p, b"<?xml?>\n<something/>\n").unwrap();
        assert!(CoberturaParser.can_parse(&p),
            "path containing 'cobertura' should match via path fallback");
    }

    #[test]
    fn parses_line_coverage() {
        let xml = br#"<?xml version="1.0"?>
<coverage>
  <packages>
    <package>
      <classes>
        <class filename="src/lib.rs">
          <lines>
            <line number="1" hits="3"/>
            <line number="2" hits="0"/>
          </lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>"#;
        let tmp = write_tmp(xml);
        let data = CoberturaParser.parse(tmp.path()).unwrap();
        let cov = data.files.get("src/lib.rs").unwrap();
        assert_eq!(*cov.get(&1).unwrap(), 3);
        assert_eq!(*cov.get(&2).unwrap(), 0);
    }
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
