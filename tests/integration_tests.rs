use omni_crap::analyzer::TreeSitterEngine;
use omni_crap::engine::{LanguageEngine, ScopeKind};

#[test]
fn rust_fixture_captures_functions() {
    let engine = TreeSitterEngine::new(None);
    let path = std::path::Path::new("tests/fixtures/rust_repo/src/lib.rs");
    let content = std::fs::read_to_string(path).unwrap();
    let scopes = engine.analyze(path, &content).unwrap();

    assert!(scopes.iter().any(|s| s.name == "add" && s.kind == ScopeKind::Function),
        "must capture 'add'; got {:?}", scopes.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert!(scopes.iter().any(|s| s.name == "factorial" && s.kind == ScopeKind::Function),
        "must capture 'factorial'");
}

#[test]
fn python_fixture_captures_functions() {
    let engine = TreeSitterEngine::new(None);
    let path = std::path::Path::new("tests/fixtures/python_repo/main.py");
    let content = std::fs::read_to_string(path).unwrap();
    let scopes = engine.analyze(path, &content).unwrap();

    assert!(scopes.iter().any(|s| s.name == "greet" && s.kind == ScopeKind::Function),
        "must capture 'greet'; got {:?}", scopes.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert!(scopes.iter().any(|s| s.name == "fibonacci" && s.kind == ScopeKind::Function),
        "must capture 'fibonacci'");
}
