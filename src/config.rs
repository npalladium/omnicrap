use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClassAction {
    /// Skip the file entirely; it will not appear in any analysis output.
    Exclude,
    /// Analyze the file but mark it in the output table.
    Flag,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClassifierConfig {
    /// Additional generated patterns (directory prefixes or file suffixes).
    #[serde(default)]
    pub generated: Vec<String>,
    /// Additional vendored patterns.
    #[serde(default)]
    pub vendored: Vec<String>,
    /// Suppress a specific built-in pattern (exact string match).
    #[serde(default)]
    pub suppress: Vec<String>,
    /// What to do with detected generated files. Default: exclude.
    #[serde(default = "default_generated_action")]
    pub generated_action: ClassAction,
    /// What to do with detected vendored files. Default: flag.
    #[serde(default = "default_vendored_action")]
    pub vendored_action: ClassAction,
}

fn default_generated_action() -> ClassAction { ClassAction::Exclude }
fn default_vendored_action()   -> ClassAction { ClassAction::Flag }

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            generated: Vec::new(),
            vendored: Vec::new(),
            suppress: Vec::new(),
            generated_action: ClassAction::Exclude,
            vendored_action: ClassAction::Flag,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    #[serde(default)]
    pub weights: RiskWeights,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub clone: CloneConfig,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default)]
    pub overrides: Vec<OverrideConfig>,
    #[serde(default)]
    pub classifier: ClassifierConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OverrideConfig {
    pub path: String,
    pub weights: PartialRiskWeights,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartialRiskWeights {
    pub structural: Option<f64>,
    pub process: Option<f64>,
    pub stability: Option<f64>,
}

fn default_max_file_size() -> u64 { 1_000_000 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CloneConfig {
    #[serde(default = "default_min_tokens")]
    pub min_tokens: usize,
}

fn default_min_tokens() -> usize { 30 }

impl Default for CloneConfig {
    fn default() -> Self {
        Self {
            min_tokens: 30,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskWeights {
    #[serde(default = "default_weight")]
    pub structural: f64,
    #[serde(default = "default_weight")]
    pub process: f64,
    #[serde(default = "default_weight")]
    pub stability: f64,
}

fn default_threshold() -> f64 { 10.0 }
fn default_weight() -> f64 { 1.0 }

impl Default for RiskWeights {
    fn default() -> Self {
        Self {
            structural: 1.0,
            process: 1.0,
            stability: 1.0,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold: 10.0,
            weights: RiskWeights::default(),
            ignore: Vec::new(),
            clone: CloneConfig::default(),
            max_file_size: 1_000_000,
            overrides: Vec::new(),
            classifier: ClassifierConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_config(dir: &std::path::Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn weights_override_matches_glob() {
        let mut cfg = Config::default();
        cfg.overrides.push(OverrideConfig {
            path: "tests/**".to_string(),
            weights: PartialRiskWeights {
                structural: Some(0.5),
                process: Some(0.0),
                stability: None,
            },
        });
        let w = cfg.get_weights_for_path("tests/foo/bar.rs");
        assert_eq!(w.structural, 0.5);
        assert_eq!(w.process, 0.0);
        assert_eq!(w.stability, 1.0); // default
    }

    #[test]
    fn weights_override_first_match_wins() {
        let mut cfg = Config::default();
        cfg.overrides.push(OverrideConfig {
            path: "tests/**".to_string(),
            weights: PartialRiskWeights { structural: Some(0.1), process: None, stability: None },
        });
        cfg.overrides.push(OverrideConfig {
            path: "tests/integration/**".to_string(),
            weights: PartialRiskWeights { structural: Some(0.9), process: None, stability: None },
        });
        // First override wins even though second is more specific
        let w = cfg.get_weights_for_path("tests/integration/foo.rs");
        assert_eq!(w.structural, 0.1);
    }

    #[test]
    fn weights_no_match_returns_defaults() {
        let cfg = Config::default();
        let w = cfg.get_weights_for_path("src/main.rs");
        assert_eq!(w.structural, 1.0);
        assert_eq!(w.process, 1.0);
        assert_eq!(w.stability, 1.0);
    }

    #[test]
    fn loads_config_from_same_dir() {
        let dir = tempdir().unwrap();
        write_config(dir.path(), ".omni-crap.toml", "threshold = 5.0\n");
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.threshold, 5.0);
    }

    #[test]
    fn walks_up_to_parent() {
        let dir = tempdir().unwrap();
        // Config lives in root; load is called from a subdirectory.
        write_config(dir.path(), ".omni-crap.toml", "threshold = 7.5\n");
        let sub = dir.path().join("src").join("utils");
        std::fs::create_dir_all(&sub).unwrap();
        let cfg = Config::load(&sub).unwrap();
        assert_eq!(cfg.threshold, 7.5);
    }

    #[test]
    fn stops_at_git_root() {
        let dir = tempdir().unwrap();
        // Mark as git root — should NOT walk further up into tmp's parent.
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        write_config(dir.path(), ".omni-crap.toml", "threshold = 3.0\n");
        let sub = dir.path().join("deep").join("path");
        std::fs::create_dir_all(&sub).unwrap();
        let cfg = Config::load(&sub).unwrap();
        assert_eq!(cfg.threshold, 3.0);
    }

    #[test]
    fn old_name_fallback() {
        let dir = tempdir().unwrap();
        // Only the legacy name present — should still load.
        write_config(dir.path(), ".omnicrap.toml", "threshold = 2.5\n");
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.threshold, 2.5);
    }

    #[test]
    fn new_name_takes_precedence_over_old() {
        let dir = tempdir().unwrap();
        write_config(dir.path(), ".omni-crap.toml", "threshold = 9.0\n");
        write_config(dir.path(), ".omnicrap.toml", "threshold = 1.0\n");
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.threshold, 9.0);
    }

    #[test]
    fn missing_config_returns_defaults() {
        let dir = tempdir().unwrap();
        // Create a .git dir so the walk stops here, no config file.
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let cfg = Config::load(dir.path()).unwrap();
        assert_eq!(cfg.threshold, 10.0); // default
    }
}

pub struct WeightMatchers(Vec<(ignore::gitignore::Gitignore, RiskWeights)>);

impl WeightMatchers {
    pub fn build(config: &Config) -> Self {
        let matchers = config.overrides.iter().map(|ovr| {
            let mut builder = ignore::gitignore::GitignoreBuilder::new(".");
            let _ = builder.add_line(None, &ovr.path);
            let gi = builder.build().unwrap_or_else(|_| {
                ignore::gitignore::GitignoreBuilder::new(".").build().unwrap()
            });
            let w = RiskWeights {
                structural: ovr.weights.structural.unwrap_or(config.weights.structural),
                process:    ovr.weights.process.unwrap_or(config.weights.process),
                stability:  ovr.weights.stability.unwrap_or(config.weights.stability),
            };
            (gi, w)
        }).collect();
        Self(matchers)
    }

    pub fn resolve(&self, path: &str, base: &RiskWeights) -> RiskWeights {
        for (gi, weights) in &self.0 {
            if gi.matched(path, false).is_ignore() {
                return weights.clone();
            }
        }
        base.clone()
    }
}

impl Config {
    pub fn get_weights_for_path(&self, path: &str) -> RiskWeights {
        for ovr in &self.overrides {
            // Simple glob-like match for now, or use a proper matcher if needed.
            // Since ignore crate is already used in WalkBuilder, we could use it here too.
            let mut builder = ignore::gitignore::GitignoreBuilder::new(".");
            let _ = builder.add_line(None, &ovr.path);
            if let Ok(ignore) = builder.build() {
                if ignore.matched(path, false).is_ignore() {
                    let mut weights = self.weights.clone();
                    if let Some(s) = ovr.weights.structural { weights.structural = s; }
                    if let Some(p) = ovr.weights.process { weights.process = p; }
                    if let Some(st) = ovr.weights.stability { weights.stability = st; }
                    return weights;
                }
            }
        }
        self.weights.clone()
    }

    pub fn load(path: &Path) -> Result<Self> {
        let mut current = if path.is_file() {
            path.parent().unwrap_or(Path::new(".")).to_path_buf()
        } else {
            path.to_path_buf()
        };

        loop {
            let config_path = current.join(".omni-crap.toml");
            if config_path.exists() {
                let content = fs::read_to_string(config_path)?;
                let config: Config = toml::from_str(&content)?;
                return Ok(config);
            }

            // Fallback to old name
            let old_config_path = current.join(".omnicrap.toml");
            if old_config_path.exists() {
                let content = fs::read_to_string(old_config_path)?;
                let config: Config = toml::from_str(&content)?;
                return Ok(config);
            }

            if current.join(".git").exists() {
                break;
            }

            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }

            #[cfg(not(windows))]
            if let Ok(home) = std::env::var("HOME") {
                if current == Path::new(&home) {
                    break;
                }
            }
        }

        Ok(Config::default())
    }
}
