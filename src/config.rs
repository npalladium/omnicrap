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
