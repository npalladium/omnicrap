mod analyzer;
mod churn;
mod coverage;

use analyzer::SemanticAnalyzer;
use churn::ChurnData;
use coverage::CoverageData;
use clap::Parser;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the LCOV coverage report (e.g., lcov.info)
    #[arg(short, long)]
    coverage: Option<PathBuf>,

    /// The time window for calculating Git churn (e.g., "90 days")
    #[arg(short, long, default_value = "90 days")]
    since: String,

    /// Minimum risk score to report
    #[arg(short, long, default_value_t = 0.0)]
    threshold: f64,

    /// Format of the output (table, json)
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Target directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,
}

struct RiskReport {
    file: String,
    function: String,
    complexity: usize,
    coverage: f64,
    churn: usize,
    risk_score: f64,
}

fn calculate_risk(complexity: usize, coverage: f64, churn: usize) -> f64 {
    let comp_factor = (complexity * complexity) as f64;
    let cov_factor = (1.0 - coverage).powi(3);
    let base_risk = comp_factor * cov_factor + (complexity as f64);
    let churn_multiplier = 1.0 + (churn as f64 + 1.0).ln();
    base_risk * churn_multiplier
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let analyzer = SemanticAnalyzer::new();
    let churn_data = ChurnData::calculate(&args.since, &args.path)?;
    let coverage_data = if let Some(path) = &args.coverage {
        Some(CoverageData::parse_lcov(path)?)
    } else {
        None
    };

    let mut reports = Vec::new();

    for entry in WalkDir::new(&args.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str());
        if !matches!(ext, Some("rs") | Some("py") | Some("js") | Some("ts")) {
            continue;
        }

        if let Ok(analysis) = analyzer.analyze_file(path) {
            // Get relative path for matching
            let relative_path = path.strip_prefix(&args.path)?.to_string_lossy().to_string();
            let file_churn = churn_data.get_churn(&relative_path);

            for func in analysis.functions {
                let func_coverage = if let Some(ref cov) = coverage_data {
                    cov.get_function_coverage(&relative_path, func.start_line, func.end_line)
                } else {
                    0.0
                };

                let risk_score = calculate_risk(func.complexity, func_coverage, file_churn);

                if risk_score >= args.threshold {
                    reports.push(RiskReport {
                        file: relative_path.clone(),
                        function: func.name,
                        complexity: func.complexity,
                        coverage: func_coverage,
                        churn: file_churn,
                        risk_score,
                    });
                }
            }
        }
    }

    reports.sort_by(|a, b| b.risk_score.partial_cmp(&a.risk_score).unwrap());

    if args.format == "json" {
        // Simple JSON output
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    } else {
        println!("{:<40} {:<30} {:<10} {:<10} {:<10} {:<10}", "File", "Function", "Comp", "Cov", "Churn", "Risk");
        println!("{}", "-".repeat(115));
        for r in reports {
            println!(
                "{:<40} {:<30} {:<10} {:<10.1}% {:<10} {:<10.2}",
                truncate(&r.file, 38),
                truncate(&r.function, 28),
                r.complexity,
                r.coverage * 100.0,
                r.churn,
                r.risk_score
            );
        }
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}

// Add Serde support for JSON output
impl serde::Serialize for RiskReport {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RiskReport", 6)?;
        state.serialize_field("file", &self.file)?;
        state.serialize_field("function", &self.function)?;
        state.serialize_field("complexity", &self.complexity)?;
        state.serialize_field("coverage", &self.coverage)?;
        state.serialize_field("churn", &self.churn)?;
        state.serialize_field("risk_score", &self.risk_score)?;
        state.end()
    }
}
