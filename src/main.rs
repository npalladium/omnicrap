use omni_crap::analyzer::TreeSitterEngine;
use omni_crap::clone_engine::CloneStore;
use omni_crap::config::Config;
use omni_crap::coverage::{CoverageParser, lcov::LcovParser, cobertura::CoberturaParser};
use omni_crap::engine::LanguageEngine;
use omni_crap::regex_engine::RegexEngine;
use omni_crap::vcs::VcsData;
use omni_crap::stats::StatsEngine;
use omni_crap::{RiskReport, RiskProfile, MetricValue, calculate_hybrid_risk, truncate};
use omni_crap::sarif;

use clap::Parser;
use std::path::PathBuf;
use std::fs;
use std::process::Command;
use std::collections::HashSet;
use rayon::prelude::*;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the coverage report (lcov.info or cobertura.xml)
    #[arg(short, long)]
    coverage: Option<PathBuf>,

    /// The time window for calculating Git churn (e.g., "90 days")
    #[arg(short, long, default_value = "90 days")]
    since: String,

    /// Minimum risk score to report (overrides config)
    #[arg(short, long)]
    threshold: Option<f64>,

    /// Format of the output (table, json, sarif)
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Only show the Change Coupling report
    #[arg(long, default_value_t = false)]
    coupling: bool,

    /// Calculate historical complexity trend (Rising Hotspots)
    #[arg(long, default_value_t = false)]
    trend: bool,

    /// Show line count statistics roll-up table
    #[arg(long, default_value_t = false)]
    stats: bool,

    /// Only analyze files changed in the current git diff (against HEAD)
    #[arg(long, default_value_t = false)]
    diff: bool,

    /// Disable clone detection (faster)
    #[arg(long, default_value_t = false)]
    no_clones: bool,

    /// Disable VCS analysis (churn, authors, coupling)
    #[arg(long, default_value_t = false)]
    no_vcs: bool,

    /// Number of threads to use for analysis (0 = auto)
    #[arg(short, long, default_value_t = 0)]
    parallelism: usize,

    /// Target directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Use classical CRAP metric (complexity^2 * (1-coverage)^3 + complexity)
    #[arg(long, default_value_t = false)]
    crap: bool,

    /// Use Churn-weighted CRAP metric (CRAP * (1 + ln(churn + 1)))
    #[arg(long, default_value_t = false)]
    ccrap: bool,

    /// Use Hybrid Z-Score Risk model (default)
    #[arg(long, default_value_t = false)]
    zcrap: bool,

    /// Minimum number of tokens to consider a duplicate (overrides config)
    #[arg(long)]
    clone_min_tokens: Option<usize>,

    /// Enable deep per-scope VCS analysis (slower)
    #[arg(long, default_value_t = false)]
    deep: bool,

    /// Number of top-N scopes to analyze deeply
    #[arg(long, default_value_t = 50)]
    deep_top_n: usize,

    /// Maximum file size to analyze in bytes (overrides config)
    #[arg(long)]
    max_file_size: Option<u64>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.parallelism > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.parallelism)
            .build_global()
            .ok(); 
    }

    let config = Config::load(&args.path)?;
    let threshold = args.threshold.unwrap_or(config.threshold);
    let min_tokens = args.clone_min_tokens.unwrap_or(config.clone.min_tokens);
    let max_file_size = args.max_file_size.unwrap_or(config.max_file_size);

    use std::io::IsTerminal;
    let _use_color = std::env::var("NO_COLOR").is_err() && std::io::stdout().is_terminal();

    let clone_store = if args.no_clones {
        None
    } else {
        Some(CloneStore::new(min_tokens))
    };

    let ts_engine = Arc::new(TreeSitterEngine::new(clone_store.clone()));
    let regex_engine = Arc::new(RegexEngine::new());
    let engines: Vec<Arc<dyn LanguageEngine>> = vec![
        ts_engine.clone(),
        regex_engine.clone(),
    ];

    let vcs_data = if args.no_vcs {
        None
    } else {
        Some(Arc::new(VcsData::calculate(&args.since, &args.path)?))
    };

    let stats_engine = Arc::new(StatsEngine::new());

    if args.coupling {
        if let Some(ref vcs) = vcs_data {
            println!("{:<40} {:<40} {:<10} {:<10}", "File 1", "File 2", "Co-Changes", "Degree");
            println!("{}", "-".repeat(105));
            for c in vcs.couplings.iter().take(50) {
                println!(
                    "{:<40} {:<40} {:<10} {:<10.0}%",
                    truncate(&c.file1, 38),
                    truncate(&c.file2, 38),
                    c.revisions,
                    c.degree * 100.0
                );
            }
        } else {
            eprintln!("Error: --coupling requires VCS analysis. Do not use --no-vcs with --coupling.");
        }
        return Ok(());
    }

    let mut changed_files = HashSet::new();
    if args.diff {
        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("HEAD")
            .current_dir(&args.path)
            .output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                changed_files.insert(line.trim().to_string());
            }
        }
        
        let output = Command::new("git")
            .arg("diff")
            .arg("--cached")
            .arg("--name-only")
            .current_dir(&args.path)
            .output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                changed_files.insert(line.trim().to_string());
            }
        }

        let output = Command::new("git")
            .arg("ls-files")
            .arg("--others")
            .arg("--exclude-standard")
            .current_dir(&args.path)
            .output()?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                changed_files.insert(line.trim().to_string());
            }
        }
    }

    let coverage_parsers: Vec<Box<dyn CoverageParser>> = vec![
        Box::new(LcovParser),
        Box::new(CoberturaParser),
    ];

    let mut coverage_data = None;
    if let Some(ref cov_path) = args.coverage {
        for parser in coverage_parsers {
            if parser.can_parse(cov_path) {
                coverage_data = Some(parser.parse(cov_path)?);
                break;
            }
        }
    }

    let past_commit_hash = if args.trend {
        let output = Command::new("git")
            .arg("rev-list")
            .arg("-n")
            .arg("1")
            .arg("--before=\"1 month ago\"")
            .arg("HEAD")
            .current_dir(&args.path)
            .output()?;
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let files: Vec<_> = ignore::WalkBuilder::new(&args.path)
        .standard_filters(true)
        .build()
        .filter_map(|e| {
            let entry = e.ok()?;
            if entry.file_type()?.is_file() {
                let len = entry.metadata().ok()?.len();
                if len > max_file_size {
                    eprintln!("Skipping {} (size {} bytes exceeds limit {} bytes)", entry.path().display(), len, max_file_size);
                    return None;
                }
                Some(entry)
            } else {
                None
            }
        })
        .collect();

    // Pass 1: Clone registration
    if let Some(ref store) = clone_store {
        files.par_iter().for_each(|entry| {
            let path = entry.path();
            let relative_path = match path.strip_prefix(&args.path) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => return,
            };
            if config.ignore.iter().any(|i| relative_path.contains(i)) {
                return;
            }
            if let Ok(content) = fs::read_to_string(path) {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(engine) = engines.iter().find(|e| e.is_supported(ext)) {
                    let _ = engine.register_clones(path, &content);
                }
            }
        });
        store.canonicalize();
    }

    // Pass 2: Analysis
    let mut reports: Vec<RiskReport> = files.par_iter().flat_map(|entry| {
        let path = entry.path();
        let relative_path = match path.strip_prefix(&args.path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Vec::new(),
        };

        if args.diff && !changed_files.contains(&relative_path) {
            return Vec::new();
        }

        if config.ignore.iter().any(|i| relative_path.contains(i)) {
            return Vec::new();
        }

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let engine = match engines.iter().find(|e| e.is_supported(ext)) {
            Some(e) => e,
            None => return Vec::new(),
        };

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let scopes = match engine.analyze(path, &content) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let file_stats = stats_engine.analyze(path, &content).unwrap_or_default();

        let (file_churn, file_authors, age_months, file_scatter, agent_ratio) = if let Some(ref vcs) = vcs_data {
            (
                vcs.get_churn(&relative_path),
                vcs.get_author_count(&relative_path),
                vcs.get_age_months(&relative_path),
                vcs.get_scatter(&relative_path),
                vcs.get_agent_ratio(&relative_path)
            )
        } else {
            (0, 0, 0.0, 0.0, 0.0)
        };

        let mut past_scopes = None;
        if args.trend && !past_commit_hash.is_empty() && file_churn > 0 {
            let historical_path = if let Some(ref vcs) = vcs_data {
                vcs.resolve_historical_path(path, "1 month ago").unwrap_or(relative_path.clone())
            } else {
                relative_path.clone()
            };

            let output = Command::new("git")
                .arg("show")
                .arg(format!("{}:{}", past_commit_hash, historical_path))
                .current_dir(&args.path)
                .output();
            if let Ok(output) = output {
                if output.status.success() {
                    let past_content = String::from_utf8_lossy(&output.stdout);
                    past_scopes = engine.analyze(path, &past_content).ok();
                }
            }
        }

        let mut local_reports = Vec::new();
        for scope in scopes {
            let complexity = *scope.metrics.get("complexity").unwrap_or(&0.0);
            let halstead = *scope.metrics.get("halstead").unwrap_or(&0.0);
            let redundancy = *scope.metrics.get("redundancy").unwrap_or(&0.0);

            let scope_coverage = if let Some(ref cov) = coverage_data {
                cov.get_function_coverage(&relative_path, scope.start_line, scope.end_line)
            } else {
                0.0
            };

            let mut trend_delta = None;
            if let Some(ref past_s) = past_scopes {
                if let Some(past_scope) = past_s.iter().find(|s| s.name == scope.name && s.kind == scope.kind) {
                    let past_comp = *past_scope.metrics.get("complexity").unwrap_or(&0.0);
                    trend_delta = Some(complexity - past_comp);
                } else {
                    trend_delta = Some(complexity); 
                }
            }

            let report = RiskReport {
                file: relative_path.clone(),
                name: scope.name,
                kind: scope.kind,
                test_kind: scope.test_kind,
                mock_count: scope.mock_count,
                clone_ratio: scope.clone_ratio,
                clone_matches: scope.clone_matches,
                start_line: scope.start_line,
                complexity,
                halstead,
                redundancy,
                coverage: scope_coverage,
                churn: file_churn,
                authors: file_authors,
                scatter: file_scatter,
                agent_ratio,
                age_months,
                trend_delta,
                risk_score: 0.0,
                percentile: 0.0,
                profile: RiskProfile {
                    structural: MetricValue { value: 0.0, z_score: 0.0, grain: "scope" },
                    process: MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
                    stability: MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
                },
                advice: String::new(),
                engine: engine.name().to_string(),
                loc: file_stats.clone(),
                weights: config.get_weights_for_path(&relative_path),
            };
            local_reports.push(report);
        }
        local_reports
    }).collect();

    // Pass 3: Scoring
    if args.crap {
        for r in reports.iter_mut() {
            r.risk_score = omni_crap::calculate_crap_risk(r.complexity, r.coverage);
            r.advice = omni_crap::generate_advice(r, &r.profile);
        }
    } else if args.ccrap {
        for r in reports.iter_mut() {
            r.risk_score = omni_crap::calculate_ccrap_risk(r.complexity, r.coverage, r.churn);
            r.advice = omni_crap::generate_advice(r, &r.profile);
        }
    } else {
        // Default to zcrap (Hybrid Z-Score)
        calculate_hybrid_risk(&mut reports, &config);
    }

    if args.stats {
        println!("{:<40} {:<10} {:<10} {:<10} {:<10}", "File", "Lines", "Code", "Comments", "Blanks");
        println!("{}", "-".repeat(85));
        let mut seen_files = HashSet::new();
        let mut total_lines = 0;
        let mut total_code = 0;
        let mut total_comments = 0;
        let mut total_blanks = 0;

        for r in &reports {
            if seen_files.insert(&r.file) {
                println!(
                    "{:<40} {:<10} {:<10} {:<10} {:<10}",
                    truncate(&r.file, 38),
                    r.loc.lines,
                    r.loc.code,
                    r.loc.comments,
                    r.loc.blanks
                );
                total_lines += r.loc.lines;
                total_code += r.loc.code;
                total_comments += r.loc.comments;
                total_blanks += r.loc.blanks;
            }
        }
        println!("{}", "-".repeat(85));
        println!(
            "{:<40} {:<10} {:<10} {:<10} {:<10}",
            "TOTAL",
            total_lines,
            total_code,
            total_comments,
            total_blanks
        );
        return Ok(());
    }

    // Pass 4: Deep analysis (Optional)
    if args.deep && vcs_data.is_some() && !args.crap {
        let vcs = vcs_data.as_ref().unwrap();
        let top_n = args.deep_top_n;
        
        // Sort first to get top-N
        reports.sort_by(|a, b| b.risk_score.total_cmp(&a.risk_score));
        
        let mut affected = false;
        for r in reports.iter_mut().take(top_n) {
            let scope_churn = vcs.get_scope_churn(&r.file, &r.name, &args.since, &args.path);
            if scope_churn > 0 {
                r.churn = scope_churn;
                r.profile.process.grain = "scope";
                affected = true;
            }
        }
        
        if affected {
            if args.ccrap {
                for r in reports.iter_mut() {
                    r.risk_score = omni_crap::calculate_ccrap_risk(r.complexity, r.coverage, r.churn);
                }
            } else {
                calculate_hybrid_risk(&mut reports, &config);
            }
        }
    }

    // Filter by threshold after scoring
    reports.retain(|r| r.risk_score >= threshold);
    reports.sort_by(|a, b| b.risk_score.total_cmp(&a.risk_score));

    let risk_header = if args.crap { "CRAP" } else if args.ccrap { "CCRAP" } else { "Risk" };

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    } else if args.format == "sarif" {
        let sarif_log = sarif::create_sarif_log(&reports);
        println!("{}", serde_json::to_string_pretty(&sarif_log).unwrap());
    } else {
        if args.trend {
            println!("{:<40} {:<30} {:<10} {:<8} {:<8} {:<10} {:<8} {:<10} {:<10} {:<10} {:<10}", "File", "Scope", "Kind", "Mocks", "Clone", "Comp", "Trend", "Struct", "Process", "Stable", risk_header);
            println!("{}", "-".repeat(170));
        } else {
            println!("{:<40} {:<30} {:<10} {:<8} {:<8} {:<10} {:<10} {:<10} {:<10} {:<10}", "File", "Scope", "Kind", "Mocks", "Clone", "Comp", "Struct", "Process", "Stable", risk_header);
            println!("{}", "-".repeat(160));
        }
        for r in reports.iter().take(100) {
            let risk_str = if args.crap || args.ccrap {
                format!("{:.1}", r.risk_score)
            } else {
                format!("{:.2} (p{:.0})", r.risk_score, r.percentile * 100.0)
            };
            let mut kind_str = format!("{:?}", r.kind);
            if let Some(tk) = r.test_kind {
                kind_str = format!("{:?}({:?})", r.kind, tk);
            }
            let mock_str = if r.mock_count > 0 { r.mock_count.to_string() } else { "-".to_string() };
            let clone_str = if r.clone_ratio > 0.0 { format!("{:.0}%", r.clone_ratio * 100.0) } else { "-".to_string() };

            if args.trend {
                let trend_str = match r.trend_delta {
                    Some(d) if d > 0.0 => format!("+{:.1}", d),
                    Some(d) if d < 0.0 => format!("{:.1}", d),
                    Some(_) => "0.0".to_string(),
                    None => "-".to_string(),
                };
                println!(
                    "{:<40} {:<30} {:<10} {:<8} {:<8} {:<10.1} {:<8} {:<10.1} {:<10.1} {:<10.1} {:<10}",
                    truncate(&r.file, 38),
                    truncate(&r.name, 28),
                    kind_str,
                    mock_str,
                    clone_str,
                    r.complexity,
                    trend_str,
                    r.profile.structural.value,
                    r.profile.process.value,
                    r.profile.stability.value,
                    risk_str
                );
            } else {
                println!(
                    "{:<40} {:<30} {:<10} {:<8} {:<8} {:<10.1} {:<10.1} {:<10.1} {:<10.1} {:<10}",
                    truncate(&r.file, 38),
                    truncate(&r.name, 28),
                    kind_str,
                    mock_str,
                    clone_str,
                    r.complexity,
                    r.profile.structural.value,
                    r.profile.process.value,
                    r.profile.stability.value,
                    risk_str
                );
            }
        }
    }

    Ok(())
}
