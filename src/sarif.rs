use crate::RiskReport;
use serde::Serialize;

const DRIVER_NAME: &str = "omni-crap";
const DRIVER_VERSION: &str = "0.1.0";
const RULE_ID: &str = "omni-crap/high-risk";
const SARIF_VERSION: &str = "2.1.0";
const SARIF_SCHEMA_URL: &str = "https://json.schemastore.org/sarif-2.1.0.json";

#[derive(Serialize)]
pub struct SarifLog {
    #[serde(rename = "$schema")]
    pub schema: &'static str,
    pub version: &'static str,
    pub runs: Vec<SarifRun>,
}

#[derive(Serialize)]
pub struct SarifRun {
    pub tool: SarifTool,
    pub results: Vec<SarifResult>,
}

#[derive(Serialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

#[derive(Serialize)]
pub struct SarifDriver {
    pub name: &'static str,
    pub version: &'static str,
    #[serde(rename = "informationUri", skip_serializing_if = "Option::is_none")]
    pub information_uri: Option<&'static str>,
    pub rules: Vec<SarifRule>,
}

#[derive(Serialize)]
pub struct SarifRule {
    pub id: &'static str,
    #[serde(rename = "shortDescription")]
    pub short_description: SarifText,
    #[serde(rename = "fullDescription")]
    pub full_description: SarifText,
    #[serde(rename = "defaultConfiguration")]
    pub default_configuration: SarifLevel,
    #[serde(rename = "helpUri", skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<&'static str>,
}

#[derive(Serialize)]
pub struct SarifText {
    pub text: &'static str,
}

#[derive(Serialize)]
pub struct SarifLevel {
    pub level: &'static str,
}

#[derive(Serialize)]
pub struct SarifResult {
    #[serde(rename = "ruleId")]
    pub rule_id: &'static str,
    pub level: &'static str,
    pub message: SarifMessage,
    pub locations: Vec<SarifLocation>,
    #[serde(rename = "relatedLocations", skip_serializing_if = "Vec::is_empty")]
    pub related_locations: Vec<SarifRelatedLocation>,
}

#[derive(Serialize)]
pub struct SarifMessage {
    pub text: String,
}

#[derive(Serialize)]
pub struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
pub struct SarifRelatedLocation {
    pub message: SarifMessage,
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
pub struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    pub artifact_location: SarifArtifactLocation,
    pub region: SarifRegion,
}

#[derive(Serialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

#[derive(Serialize)]
pub struct SarifRegion {
    #[serde(rename = "startLine")]
    pub start_line: usize,
}

pub fn create_sarif_log(reports: &[RiskReport]) -> SarifLog {
    let results: Vec<SarifResult> = reports.iter().map(build_result).collect();

    SarifLog {
        schema: SARIF_SCHEMA_URL,
        version: SARIF_VERSION,
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: build_driver(),
            },
            results,
        }],
    }
}

fn build_driver() -> SarifDriver {
    SarifDriver {
        name: DRIVER_NAME,
        version: DRIVER_VERSION,
        information_uri: None,
        rules: vec![SarifRule {
            id: RULE_ID,
            short_description: SarifText {
                text: "Omni-CRAP Risk score above threshold",
            },
            full_description: SarifText {
                text: "The Omni-CRAP score combines complexity, coverage, and VCS process metrics. \
                       Items above the threshold are considered high risk and candidates for refactoring.",
            },
            default_configuration: SarifLevel { level: "warning" },
            help_uri: None,
        }],
    }
}

fn build_result(report: &RiskReport) -> SarifResult {
    let mut related_locations = Vec::new();
    for (i, m) in report.clone_matches.iter().enumerate() {
        related_locations.push(SarifRelatedLocation {
            message: SarifMessage {
                text: format!("Clone #{} from {}", i + 1, m.other_file),
            },
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: normalize_path(&m.other_file),
                },
                region: SarifRegion {
                    start_line: m.other_start,
                },
            },
        });
    }

    SarifResult {
        rule_id: RULE_ID,
        level: "warning",
        message: SarifMessage {
            text: format!(
                "{} `{}` has risk score {:.1}x (Structural: {:.1}, Process: {:.1}, Stability: {:.1})",
                format!("{:?}", report.kind),
                report.name,
                report.risk_score,
                report.profile.structural.value,
                report.profile.process.value,
                report.profile.stability.value,
            ),
        },
        locations: vec![SarifLocation {
            physical_location: SarifPhysicalLocation {
                artifact_location: SarifArtifactLocation {
                    uri: normalize_path(&report.file),
                },
                region: SarifRegion {
                    start_line: report.start_line,
                },
            },
        }],
        related_locations,
    }
}

fn normalize_path(path_str: &str) -> String {
    path_str.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_omits_placeholder_uri() {
        let log = create_sarif_log(&[]);
        let json = serde_json::to_string(&log).unwrap();
        assert!(!json.contains("omni-crap/omni-crap"),
            "placeholder GitHub URL must not appear in SARIF output");
    }
}
