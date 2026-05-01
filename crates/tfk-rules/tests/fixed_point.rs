use tfk_protocol::ContinuationStatus;
use tfk_rules::{Atom, Rule, RuleEngine, RuleFact, Term};

fn fact(predicate: &str, args: &[&str]) -> RuleFact {
    RuleFact::new(predicate, args.iter().copied())
}

fn args(facts: Vec<&RuleFact>) -> Vec<Vec<String>> {
    facts.into_iter().map(|fact| fact.args.clone()).collect()
}

#[test]
fn derives_transitive_paths_until_fixed_point() {
    let mut engine = RuleEngine::new();
    engine.assert_fact(fact("edge", &["a", "b"]));
    engine.assert_fact(fact("edge", &["b", "c"]));
    engine.assert_fact(fact("edge", &["c", "d"]));

    engine.add_rule(Rule::new(
        Atom::new("path", [Term::var("from"), Term::var("to")]),
        vec![Atom::new("edge", [Term::var("from"), Term::var("to")])],
    ));
    engine.add_rule(Rule::new(
        Atom::new("path", [Term::var("from"), Term::var("to")]),
        vec![
            Atom::new("path", [Term::var("from"), Term::var("via")]),
            Atom::new("edge", [Term::var("via"), Term::var("to")]),
        ],
    ));

    assert_eq!(engine.evaluate(), 6);
    assert_eq!(engine.evaluate(), 0);
    assert_eq!(
        args(engine.query("path")),
        vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["a".to_string(), "c".to_string()],
            vec!["a".to_string(), "d".to_string()],
            vec!["b".to_string(), "c".to_string()],
            vec!["b".to_string(), "d".to_string()],
            vec!["c".to_string(), "d".to_string()],
        ]
    );
}

#[test]
fn eliminates_duplicate_base_and_derived_facts() {
    let mut engine = RuleEngine::new();
    engine.assert_fact(fact("edge", &["a", "b"]));
    engine.assert_fact(fact("edge", &["a", "b"]));

    let direct_path = Rule::new(
        Atom::new("path", [Term::var("from"), Term::var("to")]),
        vec![Atom::new("edge", [Term::var("from"), Term::var("to")])],
    );
    engine.add_rule(direct_path.clone());
    engine.add_rule(direct_path);

    assert_eq!(engine.query("edge").len(), 1);
    assert_eq!(engine.evaluate(), 1);
    assert_eq!(engine.query("path").len(), 1);
    assert_eq!(engine.evaluate(), 0);
}

#[test]
fn joins_body_atoms_by_shared_variable_names() {
    let mut engine = RuleEngine::new();
    engine.assert_fact(fact("assigned", &["alice", "task-1"]));
    engine.assert_fact(fact("assigned", &["bob", "task-2"]));
    engine.assert_fact(fact("risk_level", &["task-1", "high"]));
    engine.assert_fact(fact("risk_level", &["task-2", "low"]));

    engine.add_rule(Rule::new(
        Atom::new("needs_review", [Term::var("person")]),
        vec![
            Atom::new("assigned", [Term::var("person"), Term::var("task")]),
            Atom::new("risk_level", [Term::var("task"), Term::lit("high")]),
        ],
    ));

    assert_eq!(engine.evaluate(), 1);
    assert_eq!(
        args(engine.query("needs_review")),
        vec![vec!["alice".to_string()]]
    );
}

#[test]
fn core_rules_derive_active_review_and_marker_facts() {
    let mut engine = RuleEngine::with_core_rules();
    let active_status = serde_json::to_value(ContinuationStatus::Active)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    assert_eq!(active_status, "active");

    engine.assert_fact(fact("continuation", &["cont-1"]));
    engine.assert_fact(fact("continuation", &["cont-2"]));
    engine.assert_fact(fact("continuation_status", &["cont-1", &active_status]));
    engine.assert_fact(fact("continuation_status", &["cont-2", "closed"]));
    engine.assert_fact(fact("risk_level", &["cont-1", "high"]));
    engine.assert_fact(fact("risk_level", &["cont-2", "high"]));

    assert_eq!(engine.evaluate(), 3);
    assert_eq!(
        args(engine.query("active")),
        vec![vec!["cont-1".to_string()]]
    );
    assert_eq!(
        args(engine.query("needs_review")),
        vec![vec!["cont-1".to_string()]]
    );
    assert_eq!(
        args(engine.query("risk_marker")),
        vec![vec!["cont-1".to_string(), "review".to_string()]]
    );
}
