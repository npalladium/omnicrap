use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};
use anyhow::{Context, Result};

pub struct FunctionInfo {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub complexity: usize,
}

pub struct FileAnalysis {
    pub functions: Vec<FunctionInfo>,
}

pub struct SemanticAnalyzer {
    // We can add cache or language-specific parsers here
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn analyze_file(&self, path: &Path) -> Result<FileAnalysis> {
        let content = std::fs::read_to_string(path)?;
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let (mut parser, query_str) = match extension {
            "rs" => {
                let mut p = Parser::new();
                p.set_language(tree_sitter_rust::language())?;
                let q = "(function_item name: (identifier) @name) @func";
                (p, q)
            }
            "py" => {
                let mut p = Parser::new();
                p.set_language(tree_sitter_python::language())?;
                let q = "(function_definition name: (identifier) @name) @func";
                (p, q)
            }
            _ => anyhow::bail!("Unsupported language extension: {}", extension),
        };

        let tree = parser.parse(&content, None).context("Failed to parse file")?;
        let root_node = tree.root_node();

        let query = Query::new(parser.language().unwrap(), query_str)?;
        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(&query, root_node, content.as_bytes());

        let mut functions = Vec::new();
        for m in matches {
            let func_node = m.nodes_for_capture_index(1).next().unwrap();
            let name_node = m.nodes_for_capture_index(0).next().unwrap();

            let name = content[name_node.byte_range()].to_string();
            let start_line = func_node.start_position().row + 1;
            let end_line = func_node.end_position().row + 1;

            // Simple complexity heuristic: count control flow keywords in subtree
            let complexity = self.calculate_complexity(func_node, &content);

            functions.push(FunctionInfo {
                name,
                start_line,
                end_line,
                complexity,
            });
        }

        Ok(FileAnalysis {
            functions,
        })
    }

    fn calculate_complexity(&self, node: tree_sitter::Node, content: &str) -> usize {
        let mut complexity = 1;
        let mut cursor = node.walk();
        
        let decision_points = ["if", "for", "while", "match", "case", "&&", "||", "catch"];
        
        self.traverse_and_count(&mut cursor, content, &decision_points, &mut complexity);
        
        complexity
    }

    fn traverse_and_count(&self, cursor: &mut tree_sitter::TreeCursor, content: &str, points: &[&str], count: &mut usize) {
        let node = cursor.node();
        let kind = node.kind();
        
        if points.iter().any(|&p| kind.contains(p)) {
            *count += 1;
        }

        if cursor.goto_first_child() {
            self.traverse_and_count(cursor, content, points, count);
            while cursor.goto_next_sibling() {
                self.traverse_and_count(cursor, content, points, count);
            }
            cursor.goto_parent();
        }
    }
}
