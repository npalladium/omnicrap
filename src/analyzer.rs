use std::path::Path;
use arborium::tree_sitter::{self, Parser, Query, QueryCursor, StreamingIterator};
use anyhow::{Context, Result};
use crate::engine::{LanguageEngine, ScopeInfo, ScopeKind, TestKind};
use crate::clone_engine::{CloneStore, Token};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::cell::RefCell;

thread_local! {
    static PARSER_CACHE: RefCell<HashMap<&'static str, (Parser, Query)>> = RefCell::new(HashMap::new());
}

pub struct TreeSitterEngine {
    pub clone_store: Option<Arc<CloneStore>>,
}

impl TreeSitterEngine {
    pub fn new(clone_store: Option<Arc<CloneStore>>) -> Self {
        Self { clone_store }
    }
}

impl LanguageEngine for TreeSitterEngine {
    fn name(&self) -> &str {
        "tree-sitter"
    }

    fn is_supported(&self, extension: &str) -> bool {
        matches!(extension, 
            "rs" | "py" | "js" | "mjs" | "cjs" | "ts" | "tsx" | 
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | 
            "go" | "java" | "cs" | "rb" | "php" | "swift" | 
            "kt" | "scala" | "sh"
        )
    }

    fn register_clones(&self, _path: &Path, content: &str) -> Result<()> {
        let extension = _path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let relative_path = _path.to_string_lossy().to_string();

        let lang_name = match extension {
            "rs" => "rust",
            "py" => "python",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" | "tsx" => "typescript",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
            "go" => "go",
            "java" => "java",
            "cs" => "c-sharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" => "kotlin",
            "scala" => "scala",
            "sh" => "bash",
            _ => return Ok(()),
        };

        let query_str = match lang_name {
            "rust" => r#"[
                (function_item name: (identifier) @name) @func
                (impl_item type: (_) @name) @class
                (trait_item name: (identifier) @name) @class
                (struct_item name: (type_identifier) @name) @class
                (enum_item name: (type_identifier) @name) @class
            ]"#,
            "python" => "[ (function_definition name: (identifier) @name) @func (class_definition name: (identifier) @name) @class ]",
            "javascript" => "[(function_declaration name: (identifier) @name) @func (function_expression name: (identifier) @name) @func (method_definition name: (property_identifier) @name) @func (class_declaration name: (identifier) @name) @class]",
            "typescript" => "[(function_declaration name: (identifier) @name) @func (method_definition name: (property_identifier) @name) @func (class_declaration name: (identifier) @name) @class]",
            "c" | "cpp" => "[ (function_definition declarator: (function_declarator declarator: (identifier) @name)) @func (class_specifier name: (type_identifier) @name) @class ]",
            "go" => "[(function_declaration name: (identifier) @name) @func (type_declaration (type_spec name: (type_identifier) @name)) @class]",
            "java" | "c-sharp" => "[ (method_declaration name: (identifier) @name) @func (class_declaration name: (identifier) @name) @class ]",
            "ruby" => "[(method name: (identifier) @name) @func (class name: (constant) @name) @class]",
            "php" => "[(function_definition name: (name) @name) @func (class_declaration name: (name) @name) @class]",
            "swift" => "[(function_declaration name: (simple_identifier) @name) @func (class_declaration name: (type_identifier) @name) @class (struct_declaration name: (type_identifier) @name) @class]",
            "kotlin" => "[(function_declaration name: (simple_identifier) @name) @func (class_declaration name: (type_identifier) @name) @class]",
            "scala" => "[(function_definition name: (identifier) @name) @func (class_definition name: (identifier) @name) @class]",
            "bash" => "(function_definition name: (word) @name) @func",
            _ => return Ok(()),
        };

        if let Some(ref store) = self.clone_store {
            PARSER_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();

                if !cache.contains_key(lang_name) {
                    let language = arborium::get_language(lang_name)
                        .ok_or_else(|| anyhow::anyhow!("Arborium grammar not found for: {}", lang_name))?;
                    let mut parser = Parser::new();
                    parser.set_language(&language)?;
                    let query = Query::new(&language, query_str)?;
                    cache.insert(lang_name, (parser, query));
                }

                let (parser, _) = cache.get_mut(lang_name).unwrap();
                let tree = parser.parse(content, None).context("Failed to parse file")?;
                let root_node = tree.root_node();

                let all_tokens = self.extract_tokens(root_node, content);
                store.register_tokens(&relative_path, &all_tokens);
                Ok(())
            })
        } else {
            Ok(())
        }
    }

    fn analyze(&self, _path: &Path, content: &str) -> Result<Vec<ScopeInfo>> {
        let extension = _path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let relative_path = _path.to_string_lossy().to_string();

        let lang_name = match extension {
            "rs" => "rust",
            "py" => "python",
            "js" | "mjs" | "cjs" => "javascript",
            "ts" | "tsx" => "typescript",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
            "go" => "go",
            "java" => "java",
            "cs" => "c-sharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" => "kotlin",
            "scala" => "scala",
            "sh" => "bash",
            _ => anyhow::bail!("Unsupported language extension: {}", extension),
        };

        let query_str = match lang_name {
            "rust" => r#"[
                (function_item name: (identifier) @name) @func
                (impl_item type: (_) @name) @class
                (trait_item name: (identifier) @name) @class
                (struct_item name: (type_identifier) @name) @class
                (enum_item name: (type_identifier) @name) @class
            ]"#,
            "python" => "[ (function_definition name: (identifier) @name) @func (class_definition name: (identifier) @name) @class ]",
            "javascript" => "[(function_declaration name: (identifier) @name) @func (function_expression name: (identifier) @name) @func (method_definition name: (property_identifier) @name) @func (class_declaration name: (identifier) @name) @class]",
            "typescript" => "[(function_declaration name: (identifier) @name) @func (method_definition name: (property_identifier) @name) @func (class_declaration name: (identifier) @name) @class]",
            "c" | "cpp" => "[ (function_definition declarator: (function_declarator declarator: (identifier) @name)) @func (class_specifier name: (type_identifier) @name) @class ]",
            "go" => "[(function_declaration name: (identifier) @name) @func (type_declaration (type_spec name: (type_identifier) @name)) @class]",
            "java" | "c-sharp" => "[ (method_declaration name: (identifier) @name) @func (class_declaration name: (identifier) @name) @class ]",
            "ruby" => "[(method name: (identifier) @name) @func (class name: (constant) @name) @class]",
            "php" => "[(function_definition name: (name) @name) @func (class_declaration name: (name) @name) @class]",
            "swift" => "[(function_declaration name: (simple_identifier) @name) @func (class_declaration name: (type_identifier) @name) @class (struct_declaration name: (type_identifier) @name) @class]",
            "kotlin" => "[(function_declaration name: (simple_identifier) @name) @func (class_declaration name: (type_identifier) @name) @class]",
            "scala" => "[(function_definition name: (identifier) @name) @func (class_definition name: (identifier) @name) @class]",
            "bash" => "(function_definition name: (word) @name) @func",
            _ => anyhow::bail!("No query string for language: {}", lang_name),
        };

        PARSER_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();

            if !cache.contains_key(lang_name) {
                let language = arborium::get_language(lang_name)
                    .ok_or_else(|| anyhow::anyhow!("Arborium grammar not found for: {}", lang_name))?;
                let mut parser = Parser::new();
                parser.set_language(&language)?;
                let query = Query::new(&language, query_str)?;
                cache.insert(lang_name, (parser, query));
            }

            let (parser, query) = cache.get_mut(lang_name).unwrap();

            let tree = parser.parse(content, None).context("Failed to parse file")?;
            let root_node = tree.root_node();

            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(query, root_node, content.as_bytes());

            let mut scopes = Vec::new();

            // 1. Extract ALL tokens once for the whole file
            let all_tokens = self.extract_tokens(root_node, content);

            // 2. Process clones ONCE for the whole file
            let (file_clone_ratio, file_clone_matches) = if let Some(ref store) = self.clone_store {
                store.get_matches(&relative_path, &all_tokens)
            } else {
                (0.0, Vec::new())
            };

            let module_complexity = self.calculate_complexity(root_node, extension) as f64;
            let module_halstead = self.calculate_halstead(root_node, content, extension);
            
            let mut module_metrics = HashMap::new();
            module_metrics.insert("complexity".to_string(), module_complexity);
            module_metrics.insert("halstead".to_string(), module_halstead);

            scopes.push(ScopeInfo {
                name: "Global".to_string(),
                kind: ScopeKind::Module,
                test_kind: self.detect_test_kind(_path, content, root_node),
                mock_count: self.count_mocks(content, extension),
                clone_ratio: file_clone_ratio,
                clone_matches: file_clone_matches.clone(),
                start_line: 1,
                end_line: content.lines().count().max(1),
                metrics: module_metrics,
            });

            while let Some(m) = matches.next() {
                let mut scope_node = None;
                let mut name_node = None;
                let mut kind = ScopeKind::Function;

                for capture in m.captures {
                    let capture_name = query.capture_names()[capture.index as usize];
                    match capture_name {
                        "func" => {
                            scope_node = Some(capture.node);
                            kind = ScopeKind::Function;
                        }
                        "class" => {
                            scope_node = Some(capture.node);
                            kind = ScopeKind::Class;
                        }
                        "name" => {
                            name_node = Some(capture.node);
                        }
                        _ => {}
                    }
                }

                let (scope_node, name_node) = match (scope_node, name_node) {
                    (Some(s), Some(n)) => (s, n),
                    _ => continue,
                };

                let name = content[name_node.byte_range()].to_string();
                let start_line = scope_node.start_position().row + 1;
                let end_line = scope_node.end_position().row + 1;

                let complexity = self.calculate_complexity(scope_node, extension) as f64;
                let source = &content[scope_node.byte_range()];
                let halstead = self.calculate_halstead(scope_node, content, extension);
                // Efficiently filter clones for this scope using the fact that they are sorted by line
                let start_idx = file_clone_matches.partition_point(|m| m.my_start < start_line);
                let end_idx = file_clone_matches.partition_point(|m| m.my_start <= end_line);
                let scope_clone_matches = file_clone_matches[start_idx..end_idx].to_vec();
                
                // Efficiently count tokens in this scope
                let t_start_idx = all_tokens.partition_point(|t| t.line < start_line);
                let t_end_idx = all_tokens.partition_point(|t| t.line <= end_line);
                let scope_tokens_count = t_end_idx - t_start_idx;

                let scope_clone_ratio = if scope_tokens_count == 0 {
                    0.0
                } else {
                    let matched_lines: HashSet<_> = scope_clone_matches.iter().map(|m| m.my_start).collect();
                    matched_lines.len() as f64 / (end_line - start_line + 1).max(1) as f64
                };

                let mut metrics = HashMap::new();
                metrics.insert("complexity".to_string(), complexity);
                metrics.insert("halstead".to_string(), halstead);

                scopes.push(ScopeInfo {
                    name,
                    kind,
                    test_kind: self.detect_test_kind(_path, source, scope_node),
                    mock_count: self.count_mocks(source, extension),
                    clone_ratio: scope_clone_ratio,
                    clone_matches: scope_clone_matches,
                    start_line,
                    end_line,
                    metrics,
                });
            }

            Ok(scopes)
        })
    }
}

impl TreeSitterEngine {
    fn extract_tokens(&self, node: tree_sitter::Node, content: &str) -> Vec<Token> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut tokens = Vec::new();
        let mut cursor = node.walk();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            if current.child_count() == 0 {
                let kind = current.kind().to_string();
                let text = content[current.byte_range()].to_string();
                let line = current.start_position().row + 1;
                
                let is_structural = !matches!(kind.as_str(), "identifier" | "number" | "string" | "string_literal" | "integer_literal");

                let mut s = DefaultHasher::new();
                kind.hash(&mut s);
                if is_structural {
                    text.hash(&mut s);
                } else {
                    "__id__".hash(&mut s);
                }
                let hash = s.finish();

                tokens.push(Token {
                    kind,
                    content: text,
                    line,
                    is_structural,
                    hash,
                });
            } else {
                cursor.reset(current);
                if cursor.goto_first_child() {
                    let mut children = vec![cursor.node()];
                    while cursor.goto_next_sibling() {
                        children.push(cursor.node());
                    }
                    for child in children.into_iter().rev() {
                        stack.push(child);
                    }
                }
            }
        }
        tokens.sort_by_key(|t| t.line);
        tokens
    }

    fn calculate_complexity(&self, node: tree_sitter::Node, extension: &str) -> usize {
        let mut complexity = 1;
        let mut cursor = node.walk();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            let kind = current.kind();
            let is_branch = match extension {
                "rs" => matches!(kind, "if_expression" | "match_arm" | "for_expression" | "while_expression" | "&&" | "||"),
                "py" => matches!(kind, "if_statement" | "elif_clause" | "for_statement" | "while_statement" | "and" | "or"),
                "js" | "ts" => matches!(kind, "if_statement" | "switch_case" | "for_statement" | "while_statement" | "&&" | "||" | "ternary_expression"),
                "go" => matches!(kind, "if_statement" | "case_clause" | "for_statement" | "&&" | "||"),
                "java" | "cs" => matches!(kind, "if_statement" | "switch_label" | "for_statement" | "while_statement" | "&&" | "||" | "conditional_expression"),
                _ => matches!(kind, "if" | "else" | "case" | "for" | "while" | "&&" | "||"),
            };

            if is_branch {
                complexity += 1;
            }

            cursor.reset(current);
            if cursor.goto_first_child() {
                stack.push(cursor.node());
                while cursor.goto_next_sibling() {
                    stack.push(cursor.node());
                }
            }
        }
        complexity
    }

    fn calculate_halstead(&self, node: tree_sitter::Node, content: &str, extension: &str) -> f64 {
        let mut n1_kinds = HashSet::new();
        let mut n2_kinds = HashSet::new();
        let mut big_n1 = 0;
        let mut big_n2 = 0;

        let (ops_kinds, op_kinds) = match extension {
            "rs" => (
                vec!["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||", "!", "=", "+=", "-=", "*=", "/=", "&", "|", "^", "<<", ">>", "if_expression", "for_expression", "while_expression", "match_expression", "return_expression", "await_expression", "try_expression"],
                vec!["identifier", "integer_literal", "string_literal", "boolean_literal", "float_literal", "char_literal"]
            ),
            "py" => (
                vec!["+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "and", "or", "not", "=", "+=", "-=", "*=", "/=", "if_statement", "for_statement", "while_statement", "return_statement"],
                vec!["identifier", "integer", "string", "true", "false", "float"]
            ),
            "js" | "mjs" | "cjs" | "ts" | "tsx" => (
                vec!["binary_expression", "unary_expression", "assignment_expression",
                     "augmented_assignment_expression", "if_statement", "for_statement",
                     "for_in_statement", "while_statement", "do_statement",
                     "switch_statement", "return_statement", "ternary_expression",
                     "await_expression", "yield_expression", "&&", "||", "??", "!",
                     "+", "-", "*", "/", "%", "**", "==", "===", "!=", "!==",
                     "<", ">", "<=", ">=", "=", "+=", "-=", "*=", "/="],
                vec!["identifier", "number", "string", "template_string",
                     "true", "false", "null", "undefined", "this"]
            ),
            "go" => (
                vec!["binary_expression", "unary_expression", "assignment_statement",
                     "short_var_declaration", "if_statement", "for_statement",
                     "switch_statement", "select_statement", "return_statement",
                     "defer_statement", "go_statement", "range_clause",
                     "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=",
                     "&&", "||", "!", "=", ":=", "+=", "-=", "*=", "/="],
                vec!["identifier", "int_literal", "float_literal", "imaginary_literal",
                     "string_literal", "rune_literal", "true", "false", "nil"]
            ),
            "java" => (
                vec!["binary_expression", "unary_expression", "assignment_expression",
                     "if_statement", "for_statement", "enhanced_for_statement",
                     "while_statement", "do_statement", "switch_expression",
                     "return_statement", "conditional_expression", "instanceof",
                     "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=",
                     "&&", "||", "!", "=", "+=", "-=", "*=", "/="],
                vec!["identifier", "decimal_integer_literal", "hex_integer_literal",
                     "decimal_floating_point_literal", "string_literal",
                     "true", "false", "null_literal", "this"]
            ),
            "cs" => (
                vec!["binary_expression", "prefix_unary_expression", "postfix_unary_expression",
                     "assignment_expression", "if_statement", "for_statement", "foreach_statement",
                     "while_statement", "do_statement", "switch_statement", "return_statement",
                     "conditional_expression",
                     "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=",
                     "&&", "||", "!", "=", "+=", "-=", "*=", "/=", "??"],
                vec!["identifier", "integer_literal", "real_literal", "string_literal",
                     "true", "false", "null_literal", "this"]
            ),
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" => (
                vec!["binary_expression", "unary_expression", "assignment_expression",
                     "if_statement", "for_statement", "while_statement", "do_statement",
                     "switch_statement", "return_statement", "conditional_expression",
                     "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=",
                     "&&", "||", "!", "=", "+=", "-=", "*=", "/="],
                vec!["identifier", "number_literal", "string_literal",
                     "true", "false", "null"]
            ),
            _ => return self.calculate_halstead_legacy(node, content),
        };

        let mut cursor = node.walk();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            let kind = current.kind();
            if ops_kinds.contains(&kind) {
                n1_kinds.insert(kind);
                big_n1 += 1;
            } else if op_kinds.contains(&kind) {
                n2_kinds.insert(kind);
                big_n2 += 1;
            }

            cursor.reset(current);
            if cursor.goto_first_child() {
                stack.push(cursor.node());
                while cursor.goto_next_sibling() {
                    stack.push(cursor.node());
                }
            }
        }

        let n1 = n1_kinds.len() as f64;
        let n2 = n2_kinds.len() as f64;
        let n = n1 + n2;
        let big_n = (big_n1 + big_n2) as f64;

        if n > 0.0 {
            big_n * n.log2()
        } else {
            0.0
        }
    }

    fn calculate_halstead_legacy(&self, node: tree_sitter::Node, content: &str) -> f64 {
        let mut operators = HashSet::new();
        let mut operands = HashSet::new();
        let mut n1 = 0;
        let mut n2 = 0;

        let mut cursor = node.walk();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            if current.child_count() == 0 {
                let text = content[current.byte_range()].trim().to_string();
                if text.is_empty() {
                    continue;
                }

                let kind = current.kind();
                let is_operator = matches!(kind, "+" | "-" | "*" | "/" | "%" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" | "!" | "=" | "+=" | "-=" | "*=" | "/=" | "if" | "else" | "for" | "while" | "return");

                if is_operator {
                    operators.insert(text);
                    n1 += 1;
                } else {
                    operands.insert(text);
                    n2 += 1;
                }
            }

            cursor.reset(current);
            if cursor.goto_first_child() {
                stack.push(cursor.node());
                while cursor.goto_next_sibling() {
                    stack.push(cursor.node());
                }
            }
        }

        let n_unique = (operators.len() + operands.len()) as f64;
        let n_total = (n1 + n2) as f64;
        if n_unique > 0.0 {
            n_total * n_unique.log2()
        } else {
            0.0
        }
    }

    fn count_mocks(&self, source: &str, extension: &str) -> usize {
        let mock_patterns = match extension {
            "rs" => vec!["mock!", "Mock::new", "mock_all!", "automock"],
            "py" => vec!["MagicMock", "patch(", "Mock()"],
            "js" | "ts" => vec!["jest.fn()", "jest.mock(", "sinon.stub", "testDouble"],
            "go" => vec!["EXPECT()", "gomock.NewController"],
            "java" | "cs" => vec!["Mockito.mock", "new Mock<", "Substitute.For<"],
            _ => vec!["mock"],
        };

        mock_patterns.iter().map(|p| source.matches(p).count()).sum()
    }

    fn detect_test_kind(&self, path: &Path, content: &str, _node: tree_sitter::Node) -> Option<TestKind> {
        let path_str = path.to_string_lossy();
        if path_str.contains("/tests/") || path_str.contains("_test.rs") || path_str.contains(".test.") || path_str.contains(".spec.") {
            // Heuristic for E2E vs Integration vs Unit
            if content.contains("http") || content.contains("sql") || content.contains("Database") || content.contains("Network") {
                Some(TestKind::E2E)
            } else if content.contains("mock") || content.contains("Stub") {
                Some(TestKind::Integration)
            } else {
                Some(TestKind::Unit)
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halstead_js_rename_invariant() {
        let engine = TreeSitterEngine::new(None);
        let src1 = "function add(a, b) { return a + b; }";
        let src2 = "function sum(x, y) { return x + y; }";
        let s1 = engine.analyze(std::path::Path::new("t.js"), src1).unwrap();
        let s2 = engine.analyze(std::path::Path::new("t.js"), src2).unwrap();
        assert_eq!(s1[0].metrics["halstead"], s2[0].metrics["halstead"],
            "halstead must not change when identifiers are renamed");
    }

    #[test]
    fn halstead_go_rename_invariant() {
        let engine = TreeSitterEngine::new(None);
        let src1 = "package p\nfunc add(a int, b int) int { return a + b }";
        let src2 = "package p\nfunc sum(x int, y int) int { return x + y }";
        let s1 = engine.analyze(std::path::Path::new("t.go"), src1).unwrap();
        let s2 = engine.analyze(std::path::Path::new("t.go"), src2).unwrap();
        assert_eq!(s1[0].metrics["halstead"], s2[0].metrics["halstead"],
            "halstead must not change when Go identifiers are renamed");
    }
}
