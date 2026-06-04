pub mod analyzer;
pub mod classifier;
pub mod clone_engine;
pub mod config;
pub mod coverage;
pub mod engine;
#[cfg(feature = "wip")]
pub mod forge;
pub mod languages;
pub mod regex_engine;
pub mod sarif;
pub mod stats;
pub mod vcs;

use std::collections::HashSet;
use serde::Serialize;
use engine::{ScopeKind, TestKind};
use clone_engine::CloneMatch;
use config::{Config, RiskWeights};
use stats::FileStats;

#[derive(Debug, Clone, Serialize)]
pub struct MetricValue {
    pub value: f64,
    pub z_score: f64,
    pub grain: &'static str, // "file" | "scope"
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskProfile {
    pub structural: MetricValue,
    pub process: MetricValue,
    pub stability: MetricValue,
}

#[derive(Debug, Clone, Serialize)]
pub struct RiskReport {
    pub file: String,
    pub name: String,
    pub kind: ScopeKind,
    pub test_kind: Option<TestKind>,
    pub mock_count: usize,
    pub clone_ratio: f64,
    pub clone_matches: Vec<CloneMatch>,
    pub start_line: usize,
    pub complexity: f64,
    pub halstead: f64,
    pub redundancy: f64,
    pub coverage: f64,
    pub churn: usize,
    pub authors: usize,
    pub scatter: f64,
    pub agent_ratio: f64,
    pub age_months: f64,
    pub trend_delta: Option<f64>,
    pub risk_score: f64,
    pub percentile: f64,
    pub profile: RiskProfile,
    pub advice: String,
    pub engine: String,
    pub loc: FileStats,
    /// Set to Some(_) when the file was classified as generated/vendored with action=Flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_class: Option<classifier::FileClass>,
    #[serde(skip)]
    pub weights: RiskWeights,
}

pub fn generate_advice(report: &RiskReport, profile: &RiskProfile) -> String {
    if !report.risk_score.is_finite() || !profile.structural.value.is_finite() || !profile.process.value.is_finite() || !profile.stability.value.is_finite() {
        return "DEGENERATE: Risk metrics produced non-finite values. Check input data (complexity, churn, etc.).".to_string();
    }

    if !report.clone_matches.is_empty() && report.clone_ratio > 0.3 {
        let other_files: HashSet<_> = report.clone_matches.iter().map(|m| &m.other_file).collect();
        let other_list: Vec<_> = other_files.into_iter().take(3).collect();
        return format!("CLONE: High duplication ({:.0}%). Also found in: {}. Consider refactoring into a shared helper.", report.clone_ratio * 100.0, other_list.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(". "));
    }

    if let Some(tk) = report.test_kind {
        if report.mock_count > 5 {
            return "HEAVY MOCKING: This test relies on many mocks. Consider favoring 'natural extent' and testing against real dependencies where possible.".to_string();
        }

        match tk {
            TestKind::E2E => if report.complexity > 10.0 {
                return "IMPURE TEST: This E2E test is becoming complex. Consider pushing logic to pure integration or unit tests.".to_string();
            },
            TestKind::Integration => if report.complexity > 20.0 {
                return "EXTENT RISK: High-extent integration test is too complex. Refactor to simpler unit tests.".to_string();
            },
            TestKind::Unit => if report.complexity > 15.0 {
                return "TEST COMPLEXITY: Unit test should be simple. High complexity suggests testing implementation details too much.".to_string();
            },
        }
    }

    if profile.structural.value > profile.process.value * 2.0 {
        if report.coverage < 0.5 {
            return "TESTING DEBT: Structural risk is high. Write unit tests before refactoring.".to_string();
        } else {
            return "COMPLEXITY: Consider breaking down this scope into smaller parts.".to_string();
        }
    }
    if profile.process.value > profile.structural.value * 1.5 {
        if report.authors == 1 {
            return "KNOWLEDGE SILO: Highly volatile code with only one author. Shared review needed.".to_string();
        } else {
            return "HOTSPOT: High churn area. Ensure changes are strictly necessary.".to_string();
        }
    }
    if report.redundancy > 0.3 {
        return "BOILERPLATE: High redundancy detected. Consolidate logic to reduce surface area.".to_string();
    }

    if report.agent_ratio > 0.4 && report.risk_score > 2.0 {
        return "AI GENERATED: Significant portion of this code was authored by agents. Review for correctness.".to_string();
    }

    "MAINTAIN: Keep an eye on this area for rising complexity.".to_string()
}

pub struct Distribution {
    pub mean: f64,
    pub stddev: f64,
}

impl Distribution {
    pub fn new(values: &[f64]) -> Self {
        let n = values.len() as f64;
        if n == 0.0 { return Self { mean: 0.0, stddev: 1.0 }; }
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let stddev = variance.sqrt().max(0.0001);
        Self { mean, stddev }
    }

    pub fn z_score(&self, value: f64) -> f64 {
        (value - self.mean) / self.stddev
    }
}

pub fn calculate_crap_risk(complexity: f64, coverage: f64) -> f64 {
    let comp_factor = complexity.powi(2);
    let cov_factor = (1.0 - coverage).powi(3);
    comp_factor * cov_factor + complexity
}

pub fn calculate_ccrap_risk(complexity: f64, coverage: f64, churn: usize) -> f64 {
    let crap = calculate_crap_risk(complexity, coverage);
    let churn_multiplier = 1.0 + (churn as f64 + 1.0).ln();
    crap * churn_multiplier
}

pub fn calculate_hybrid_risk(
    reports: &mut [RiskReport],
    _config: &Config
) {
    if reports.is_empty() { return; }

    let weighted_complexities: Vec<f64> = reports.iter().map(|r| {
        r.complexity * (1.0 + (1.0 - r.coverage).powi(3))
    }).collect();
    let halsteads: Vec<f64> = reports.iter().map(|r| r.halstead).collect();
    let clones: Vec<f64> = reports.iter().map(|r| r.clone_ratio).collect();
    let churns: Vec<f64> = reports.iter().map(|r| r.churn as f64).collect();
    let authors: Vec<f64> = reports.iter().map(|r| r.authors as f64).collect();
    let scatters: Vec<f64> = reports.iter().map(|r| r.scatter).collect();
    let redundancies: Vec<f64> = reports.iter().map(|r| r.redundancy).collect();
    let ages: Vec<f64> = reports.iter().map(|r| r.age_months).collect();

    let d_comp = Distribution::new(&weighted_complexities);
    let d_hal = Distribution::new(&halsteads);
    let d_clone = Distribution::new(&clones);
    let d_churn = Distribution::new(&churns);
    let d_auth = Distribution::new(&authors);
    let d_scat = Distribution::new(&scatters);
    let d_red = Distribution::new(&redundancies);
    let d_age = Distribution::new(&ages);

    for (i, r) in reports.iter_mut().enumerate() {
        let z_comp = d_comp.z_score(weighted_complexities[i]) * r.weights.structural;
        let z_hal = d_hal.z_score(r.halstead) * r.weights.structural;
        let z_clone = d_clone.z_score(r.clone_ratio) * r.weights.structural;
        
        let z_churn = d_churn.z_score(r.churn as f64) * r.weights.process;
        let z_auth = d_auth.z_score(r.authors as f64) * r.weights.process;
        let z_scat = d_scat.z_score(r.scatter) * r.weights.process;

        let z_red = d_red.z_score(r.redundancy) * r.weights.stability;
        let z_age = d_age.z_score(r.age_months) * r.weights.stability;

        let structural = z_comp + z_hal + z_clone;
        let process = z_churn + z_auth + z_scat;
        let stability = z_red + z_age;

        r.profile.structural = MetricValue { value: structural, z_score: structural, grain: "scope" };
        r.profile.process = MetricValue { value: process, z_score: process, grain: "file" };
        r.profile.stability = MetricValue { value: stability, z_score: stability, grain: "file" };

        r.risk_score = (1.0 + structural) * (1.0 + process) / (1.0 + stability).max(0.1);
        
        if !r.risk_score.is_finite() {
            r.risk_score = 0.0;
        }
    }

    // Calculate percentiles
    let mut scores: Vec<f64> = reports.iter().map(|r| r.risk_score).collect();
    scores.sort_by(|a, b| a.total_cmp(b));
    
    for r in reports.iter_mut() {
        let pos = scores.binary_search_by(|s| s.total_cmp(&r.risk_score)).unwrap_or(0);
        r.percentile = (pos as f64) / (scores.len() as f64);
        r.advice = generate_advice(r, &r.profile);
    }
}

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ScopeKind;

    #[test]
    fn test_distribution() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let d = Distribution::new(&values);
        assert_eq!(d.mean, 3.0);
        assert!(d.stddev > 0.0);
        assert_eq!(d.z_score(3.0), 0.0);
        assert!(d.z_score(5.0) > 0.0);
        assert!(d.z_score(1.0) < 0.0);
    }

    #[test]
    fn test_hybrid_risk_monotonicity() {
        let r1 = RiskReport {
            file: "f1.rs".to_string(),
            name: "s1".to_string(),
            kind: ScopeKind::Function,
            test_kind: None,
            mock_count: 0,
            clone_ratio: 0.1,
            clone_matches: vec![],
            start_line: 1,
            complexity: 10.0,
            halstead: 100.0,
            redundancy: 0.1,
            coverage: 0.5,
            churn: 10,
            authors: 2,
            scatter: 1.0,
            agent_ratio: 0.0,
            age_months: 6.0,
            trend_delta: None,
            risk_score: 0.0,
            percentile: 0.0,
            profile: RiskProfile {
                structural: MetricValue { value: 0.0, z_score: 0.0, grain: "scope" },
                process: MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
                stability: MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
            },
            advice: "".to_string(),
            engine: "test".to_string(),
            loc: FileStats::default(),
            file_class: None,
            weights: RiskWeights::default(),
        };

        let mut r2 = r1.clone();
        r2.complexity = 20.0; // Higher complexity should mean higher risk

        let mut reports = vec![r1, r2];
        let config = Config::default();
        calculate_hybrid_risk(&mut reports, &config);

        assert!(reports[1].risk_score > reports[0].risk_score);
    }
}
