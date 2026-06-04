use dashmap::DashMap;
use std::sync::Arc;
use std::collections::HashMap;

/// A high-performance, thread-safe store for code block hashes.
pub struct CloneStore {
    // Maps rolling hash -> Vec<CloneEntry>
    buckets: DashMap<u64, Vec<CloneEntry>>,
    // Finalized matches: rolling hash -> Vec<FinalizedClone>
    finalized: DashMap<u64, Vec<FinalizedClone>>,
    // Minimum number of tokens to consider a duplicate
    min_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct CloneEntry {
    pub location: CloneLocation,
    pub token_hashes: Vec<u64>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CloneLocation {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct FinalizedClone {
    pub token_hashes: Vec<u64>,
    pub canonical: CloneLocation,
    pub others: Vec<CloneLocation>,
}

impl CloneStore {
    pub fn new(min_tokens: usize) -> Arc<Self> {
        Arc::new(Self {
            buckets: DashMap::new(),
            finalized: DashMap::new(),
            min_tokens,
        })
    }

    pub fn min_tokens(&self) -> usize {
        self.min_tokens
    }

    /// Pass 1: Register all token windows in parallel.
    pub fn register_tokens(&self, file_path: &str, tokens: &[Token]) {
        if tokens.len() < self.min_tokens {
            return;
        }

        let p = 31u64;
        let mut current_hash = 0u64;
        let mut power = 1u64;

        for i in 0..self.min_tokens {
            current_hash = current_hash.wrapping_mul(p).wrapping_add(tokens[i].hash);
            if i > 0 {
                power = power.wrapping_mul(p);
            }
        }

        for i in 0..=(tokens.len() - self.min_tokens) {
            if i > 0 {
                let leading = tokens[i - 1].hash.wrapping_mul(power);
                current_hash = current_hash.wrapping_sub(leading);
                current_hash = current_hash.wrapping_mul(p).wrapping_add(tokens[i + self.min_tokens - 1].hash);
            }

            let entry = CloneEntry {
                location: CloneLocation {
                    file: file_path.to_string(),
                    start_line: tokens[i].line,
                    end_line: tokens[i + self.min_tokens - 1].line,
                },
                token_hashes: tokens[i..i + self.min_tokens].iter().map(|t| t.hash).collect(),
            };

            self.buckets.entry(current_hash).or_insert_with(Vec::new).push(entry);
        }
    }

    /// Pass 2: Serial canonicalization.
    pub fn canonicalize(&self) {
        for mut bucket in self.buckets.iter_mut() {
            let rolling_hash = *bucket.key();
            let entries = bucket.value_mut();
            
            let mut groups: HashMap<Vec<u64>, Vec<CloneLocation>> = HashMap::new();
            for entry in entries.drain(..) {
                groups.entry(entry.token_hashes).or_default().push(entry.location);
            }

            for (token_hashes, mut locations) in groups {
                if locations.len() > 1 {
                    locations.sort();
                    let canonical = locations.remove(0);
                    self.finalized.entry(rolling_hash).or_default().push(FinalizedClone {
                        token_hashes,
                        canonical,
                        others: locations,
                    });
                }
            }
        }
        self.buckets.clear();
    }

    /// Pass 3: Get matches for a file in parallel.
    pub fn get_matches(&self, file_path: &str, tokens: &[Token]) -> (f64, Vec<CloneMatch>) {
        if tokens.len() < self.min_tokens {
            return (0.0, Vec::new());
        }

        let mut duplicated_indices = vec![false; tokens.len()];
        let mut matches = Vec::new();

        let p = 31u64;
        let mut current_hash = 0u64;
        let mut power = 1u64;

        for i in 0..self.min_tokens {
            current_hash = current_hash.wrapping_mul(p).wrapping_add(tokens[i].hash);
            if i > 0 {
                power = power.wrapping_mul(p);
            }
        }

        for i in 0..=(tokens.len() - self.min_tokens) {
            if i > 0 {
                let leading = tokens[i - 1].hash.wrapping_mul(power);
                current_hash = current_hash.wrapping_sub(leading);
                current_hash = current_hash.wrapping_mul(p).wrapping_add(tokens[i + self.min_tokens - 1].hash);
            }

            if let Some(finalized_clones) = self.finalized.get(&current_hash) {
                for fc in finalized_clones.iter() {
                    // Exact match check
                    if tokens[i..i+self.min_tokens].iter().zip(&fc.token_hashes).all(|(t, h)| t.hash == *h) {
                        let current_loc = CloneLocation {
                             file: file_path.to_string(),
                             start_line: tokens[i].line,
                             end_line: tokens[i + self.min_tokens - 1].line,
                        };
                        
                        let (other_file, other_start) = if current_loc == fc.canonical {
                            (&fc.others[0].file, fc.others[0].start_line)
                        } else {
                            (&fc.canonical.file, fc.canonical.start_line)
                        };

                        matches.push(CloneMatch {
                            other_file: other_file.clone(),
                            other_start,
                            my_start: tokens[i].line,
                        });
                        
                        for j in 0..self.min_tokens {
                            duplicated_indices[i + j] = true;
                        }
                        break; 
                    }
                }
            }
        }

        let duplicated_count = duplicated_indices.iter().filter(|&&d| d).count();
        let ratio = duplicated_count as f64 / tokens.len() as f64;

        (ratio, matches)
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: String,
    pub content: String,
    pub line: usize,
    pub is_structural: bool,
    pub hash: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CloneMatch {
    pub other_file: String,
    pub other_start: usize,
    pub my_start: usize,
}
