use std::collections::HashMap;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SccLanguage {
    #[serde(default)]
    pub complexitychecks: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub line_comment: Vec<String>,
    #[serde(default)]
    pub multi_line: Vec<Vec<String>>,
    #[serde(default)]
    pub quotes: Vec<QuotePair>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuotePair {
    pub start: String,
    pub end: String,
}

pub struct LanguageDatabase {
    pub extensions: HashMap<String, String>,
    pub languages: HashMap<String, SccLanguage>,
}

static INSTANCE: OnceLock<LanguageDatabase> = OnceLock::new();

impl LanguageDatabase {
    pub fn get() -> &'static Self {
        INSTANCE.get_or_init(Self::new)
    }

    fn new() -> Self {
        let json_data = include_str!("languages.json");
        let languages: HashMap<String, SccLanguage> = serde_json::from_str(json_data).expect("Failed to parse languages.json");
        
        let mut extensions = HashMap::new();
        for (name, lang) in &languages {
            for ext in &lang.extensions {
                extensions.insert(ext.clone(), name.clone());
            }
        }

        Self {
            extensions,
            languages,
        }
    }

    pub fn get_by_extension(&self, ext: &str) -> Option<&SccLanguage> {
        self.extensions.get(ext).and_then(|name| self.languages.get(name))
    }
}
