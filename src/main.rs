use omni_crap::analyzer::TreeSitterEngine;
use omni_crap::classifier::{FileClass, FileClassifier, Classification, read_head_bytes};
use omni_crap::clone_engine::CloneStore;
use omni_crap::config::{ClassAction, Config};
use omni_crap::coverage::{CoverageParser, lcov::LcovParser, cobertura::CoberturaParser};
use omni_crap::engine::LanguageEngine;
use omni_crap::regex_engine::RegexEngine;
use omni_crap::vcs::VcsData;
use omni_crap::stats::StatsEngine;
use omni_crap::{RiskReport, RiskProfile, MetricValue, calculate_hybrid_risk};
use omni_crap::sarif;

use clap::Parser;
use comfy_table::{Table, ContentArrangement, presets};
use std::path::PathBuf;
use std::fs;
use std::process::Command;
use std::collections::{HashSet, HashMap};
use rayon::prelude::*;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Identify high-risk code via complexity, churn, and coverage analysis",
    long_about = "omni-crap scores every function and method in your codebase using a hybrid \
Z-score model that weighs structural complexity, process history (churn, author spread), \
and stability signals (coverage, clones, AI-generated code).\n\n\
The result is a ranked table of the riskiest scopes, so you know exactly where to focus \
refactoring, review, and testing effort.",
    after_help = "EXAMPLES:
  omni-crap                                  Analyze current directory
  omni-crap --coverage lcov.info             Include line-coverage data
  omni-crap --format json > report.json      Machine-readable JSON output
  omni-crap --coupling                       Change-coupling between files
  omni-crap --stats                          Line-count roll-up per file
  omni-crap --diff                           Only score changed files (fast CI use)
  omni-crap --trend --since \"180 days\"     Flag rising complexity hotspots
  omni-crap --crap --coverage lcov.info      Classical CRAP metric
  omni-crap --generated-report               Show all detected generated/vendored files"
)]
struct Args {
    /// Target directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,

    // ── Analysis ─────────────────────────────────────────────────────────────

    /// Path to a coverage report (lcov.info or cobertura.xml)
    #[arg(short, long, help_heading = "Analysis")]
    coverage: Option<PathBuf>,

    /// Git history window for churn calculation
    #[arg(short, long, default_value = "90 days", help_heading = "Analysis")]
    since: String,

    /// Restrict analysis to files in the current git diff (staged, unstaged, and untracked)
    #[arg(long, default_value_t = false, help_heading = "Analysis")]
    diff: bool,

    /// Refine churn for top-N scopes using per-function git blame (slower)
    #[arg(long, default_value_t = false, help_heading = "Analysis")]
    deep: bool,

    /// How many top-ranked scopes to refine when --deep is active
    #[arg(long, default_value_t = 50, help_heading = "Analysis")]
    deep_top_n: usize,

    /// Skip files larger than this many bytes
    #[arg(long, help_heading = "Analysis")]
    max_file_size: Option<u64>,

    // ── Output ────────────────────────────────────────────────────────────────

    /// Output format: table, json, sarif
    #[arg(short, long, default_value = "table", help_heading = "Output")]
    format: String,

    /// Hide scopes with a risk score below this value
    #[arg(short, long, help_heading = "Output")]
    threshold: Option<f64>,

    /// Show the change-coupling report instead of the risk table
    #[arg(long, default_value_t = false, help_heading = "Output")]
    coupling: bool,

    /// Show a line-count roll-up (lines, code, comments, blanks)
    #[arg(long, default_value_t = false, help_heading = "Output")]
    stats: bool,

    /// Append a complexity-trend column (compares against 1 month ago)
    #[arg(long, default_value_t = false, help_heading = "Output")]
    trend: bool,

    /// Show a table of all detected generated/vendored files with their detection reason
    #[arg(long, default_value_t = false, help_heading = "Output")]
    generated_report: bool,

    // ── Risk model ────────────────────────────────────────────────────────────

    /// Classical CRAP: complexity² × (1 − coverage)³ + complexity
    #[arg(long, default_value_t = false, help_heading = "Risk model")]
    crap: bool,

    /// Churn-weighted CRAP: CRAP × (1 + ln(churn + 1))
    #[arg(long, default_value_t = false, help_heading = "Risk model")]
    ccrap: bool,

    /// Hybrid Z-score model combining structural, process, and stability signals (default)
    #[arg(long, default_value_t = false, help_heading = "Risk model")]
    zcrap: bool,

    // ── Performance ──────────────────────────────────────────────────────────

    /// Worker threads (0 = one per logical CPU)
    #[arg(short, long, default_value_t = 0, help_heading = "Performance")]
    parallelism: usize,

    /// Skip VCS analysis (no churn, author count, or coupling data)
    #[arg(long, default_value_t = false, help_heading = "Performance")]
    no_vcs: bool,

    /// Skip clone/duplicate detection
    #[arg(long, default_value_t = false, help_heading = "Performance")]
    no_clones: bool,

    /// Minimum token run to flag as a duplicate (overrides config)
    #[arg(long, help_heading = "Performance")]
    clone_min_tokens: Option<usize>,
}

fn make_table() -> Table {
    let mut table = Table::new();
    table
        .load_preset(presets::UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table
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

    let classifier = FileClassifier::new(config.classifier.clone(), &args.path);

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
            let mut table = make_table();
            table.set_header(vec!["File 1", "File 2", "Co-Changes", "Degree"]);
            for c in vcs.couplings.iter().take(50) {
                table.add_row(vec![
                    c.file1.clone(),
                    c.file2.clone(),
                    c.revisions.to_string(),
                    format!("{:.0}%", c.degree * 100.0),
                ]);
            }
            println!("{table}");
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

    // ── Walk: collect and classify all files ──────────────────────────────────

    let all_files: Vec<_> = ignore::WalkBuilder::new(&args.path)
        .standard_filters(true)
        .build()
        .filter_map(|e| {
            let entry = e.ok()?;
            if entry.file_type()?.is_file() {
                let len = entry.metadata().ok()?.len();
                if len > max_file_size {
                    eprintln!("Skipping {} (size {} bytes exceeds limit {} bytes)",
                        entry.path().display(), len, max_file_size);
                    return None;
                }
                Some(entry)
            } else {
                None
            }
        })
        .collect();

    // Classification pass (sequential, runs before parallel analysis).
    // Reads only the first 512 bytes per file for header scanning.
    let mut gen_excluded  = 0usize;
    let mut vendor_excluded = 0usize;
    // path → classification for files that are *flagged* (not excluded)
    let mut flagged: HashMap<String, FileClass> = HashMap::new();
    // full record of all classified files (for --generated-report)
    let mut classified_log: Vec<(String, Classification)> = Vec::new();
    // paths to skip in analysis passes
    let mut excluded_paths: HashSet<String> = HashSet::new();

    for entry in &all_files {
        let path = entry.path();
        let relative_path = match path.strip_prefix(&args.path) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if config.ignore.iter().any(|i| relative_path.contains(i.as_str())) {
            continue;
        }

        let head = read_head_bytes(path, 512);
        if let Some(cls) = classifier.classify(&relative_path, &head) {
            match cls.action {
                ClassAction::Exclude => {
                    excluded_paths.insert(relative_path.clone());
                    match cls.class {
                        FileClass::Generated => gen_excluded += 1,
                        FileClass::Vendored  => vendor_excluded += 1,
                    }
                }
                ClassAction::Flag => {
                    flagged.insert(relative_path.clone(), cls.class);
                }
            }
            classified_log.push((relative_path, cls));
        }
    }

    // Short-circuit: show generated/vendored report and exit.
    if args.generated_report {
        if classified_log.is_empty() {
            println!("No generated or vendored files detected.");
        } else {
            let mut table = make_table();
            table.set_header(vec!["File", "Type", "Action", "Signal", "Pattern"]);
            for (path, cls) in &classified_log {
                let type_str = match cls.class {
                    FileClass::Generated => "generated",
                    FileClass::Vendored  => "vendored",
                };
                let action_str = match cls.action {
                    ClassAction::Exclude => "exclude",
                    ClassAction::Flag    => "flag",
                };
                table.add_row(vec![
                    path.clone(),
                    type_str.to_string(),
                    action_str.to_string(),
                    cls.reason.to_string(),
                    cls.pattern.clone(),
                ]);
            }
            println!("{table}");
        }
        return Ok(());
    }

    // Filter excluded files out of the working set for analysis passes.
    let files: Vec<_> = all_files.into_iter()
        .filter(|entry| {
            let path = entry.path();
            match path.strip_prefix(&args.path) {
                Ok(rel) => !excluded_paths.contains(rel.to_string_lossy().as_ref()),
                Err(_)  => true,
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
            if config.ignore.iter().any(|i| relative_path.contains(i.as_str())) {
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

        if config.ignore.iter().any(|i| relative_path.contains(i.as_str())) {
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

        let file_class = flagged.get(&relative_path).copied();

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
            let halstead   = *scope.metrics.get("halstead").unwrap_or(&0.0);
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
                    process:    MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
                    stability:  MetricValue { value: 0.0, z_score: 0.0, grain: "file" },
                },
                advice: String::new(),
                engine: engine.name().to_string(),
                loc: file_stats.clone(),
                file_class,
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
        calculate_hybrid_risk(&mut reports, &config);
    }

    if args.stats {
        let mut table = make_table();
        table.set_header(vec!["File", "Lines", "Code", "Comments", "Blanks"]);
        let mut seen_files = HashSet::new();
        let mut total_lines    = 0usize;
        let mut total_code     = 0usize;
        let mut total_comments = 0usize;
        let mut total_blanks   = 0usize;

        for r in &reports {
            if seen_files.insert(&r.file) {
                table.add_row(vec![
                    r.file.clone(),
                    r.loc.lines.to_string(),
                    r.loc.code.to_string(),
                    r.loc.comments.to_string(),
                    r.loc.blanks.to_string(),
                ]);
                total_lines    += r.loc.lines;
                total_code     += r.loc.code;
                total_comments += r.loc.comments;
                total_blanks   += r.loc.blanks;
            }
        }
        table.add_row(vec![
            "TOTAL".to_string(),
            total_lines.to_string(),
            total_code.to_string(),
            total_comments.to_string(),
            total_blanks.to_string(),
        ]);
        println!("{table}");

        print_exclusion_summary(gen_excluded, vendor_excluded);
        return Ok(());
    }

    // Pass 4: Deep analysis (Optional)
    if args.deep && vcs_data.is_some() && !args.crap {
        let vcs = vcs_data.as_ref().unwrap();
        let top_n = args.deep_top_n;

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
    let has_flagged = reports.iter().any(|r| r.file_class.is_some());

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    } else if args.format == "sarif" {
        let sarif_log = sarif::create_sarif_log(&reports);
        println!("{}", serde_json::to_string_pretty(&sarif_log).unwrap());
    } else {
        let mut table = make_table();

        // Build header row, inserting "Type" when flagged files are present.
        let mut headers: Vec<&str> = Vec::new();
        if has_flagged { headers.push("Type"); }
        headers.extend_from_slice(&["File", "Scope", "Kind", "Mocks", "Clone", "Comp"]);
        if args.trend { headers.push("Trend"); }
        headers.extend_from_slice(&["Struct", "Process", "Stable", risk_header]);
        table.set_header(headers);

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
            let mock_str  = if r.mock_count > 0  { r.mock_count.to_string() }      else { "-".to_string() };
            let clone_str = if r.clone_ratio > 0.0 { format!("{:.0}%", r.clone_ratio * 100.0) } else { "-".to_string() };
            let trend_str = r.trend_delta.map(|d| {
                if d > 0.0       { format!("+{:.1}", d) }
                else if d < 0.0  { format!("{:.1}", d) }
                else             { "0.0".to_string() }
            });

            let mut row: Vec<String> = Vec::new();
            if has_flagged {
                row.push(match r.file_class {
                    Some(FileClass::Generated) => "gen".to_string(),
                    Some(FileClass::Vendored)  => "vendor".to_string(),
                    None => "-".to_string(),
                });
            }
            row.push(r.file.clone());
            row.push(r.name.clone());
            row.push(kind_str);
            row.push(mock_str);
            row.push(clone_str);
            row.push(format!("{:.1}", r.complexity));
            if args.trend {
                row.push(trend_str.unwrap_or_else(|| "-".to_string()));
            }
            row.push(format!("{:.1}", r.profile.structural.value));
            row.push(format!("{:.1}", r.profile.process.value));
            row.push(format!("{:.1}", r.profile.stability.value));
            row.push(risk_str);

            table.add_row(row);
        }

        println!("{table}");
    }

    print_exclusion_summary(gen_excluded, vendor_excluded);

    Ok(())
}

fn print_exclusion_summary(generated: usize, vendored: usize) {
    if generated > 0 || vendored > 0 {
        eprintln!("Skipped {} generated, {} vendored files.", generated, vendored);
    }
}
