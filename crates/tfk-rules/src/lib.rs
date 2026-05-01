use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

type Bindings = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuleFact {
    pub predicate: String,
    pub args: Vec<String>,
}

impl RuleFact {
    pub fn new<I, S>(predicate: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            predicate: predicate.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Term {
    Var(String),
    Lit(String),
}

impl Term {
    pub fn var(name: impl Into<String>) -> Self {
        Self::Var(name.into())
    }

    pub fn lit(value: impl Into<String>) -> Self {
        Self::Lit(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Atom {
    pub predicate: String,
    pub terms: Vec<Term>,
}

impl Atom {
    pub fn new<I, T>(predicate: impl Into<String>, terms: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Term>,
    {
        Self {
            predicate: predicate.into(),
            terms: terms.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub head: Atom,
    pub body: Vec<Atom>,
}

impl Rule {
    pub fn new(head: Atom, body: Vec<Atom>) -> Self {
        Self { head, body }
    }
}

#[derive(Debug, Default)]
pub struct RuleEngine {
    facts: Vec<RuleFact>,
    fact_set: BTreeSet<RuleFact>,
    rules: Vec<Rule>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_core_rules() -> Self {
        let mut engine = Self::new();
        engine.add_core_rules();
        engine
    }

    pub fn assert_fact(&mut self, fact: RuleFact) -> bool {
        self.insert_fact(fact)
    }

    pub fn assert_fact_parts<I, S>(&mut self, predicate: impl Into<String>, args: I) -> bool
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.assert_fact(RuleFact::new(predicate, args))
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    pub fn add_rules<I>(&mut self, rules: I)
    where
        I: IntoIterator<Item = Rule>,
    {
        self.rules.extend(rules);
    }

    pub fn add_core_rules(&mut self) {
        self.add_rules(core_rules());
    }

    pub fn evaluate(&mut self) -> usize {
        let mut added_total = 0;

        loop {
            let snapshot = self.facts.clone();
            let mut added_this_round = 0;
            let derived: Vec<_> = self
                .rules
                .iter()
                .flat_map(|rule| derive_rule(rule, &snapshot))
                .collect();

            for fact in derived {
                if self.insert_fact(fact) {
                    added_this_round += 1;
                }
            }

            if added_this_round == 0 {
                break;
            }

            added_total += added_this_round;
        }

        added_total
    }

    pub fn query(&self, predicate: &str) -> Vec<&RuleFact> {
        self.facts
            .iter()
            .filter(|fact| fact.predicate == predicate)
            .collect()
    }

    pub fn facts(&self) -> &[RuleFact] {
        &self.facts
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    fn insert_fact(&mut self, fact: RuleFact) -> bool {
        if !self.fact_set.insert(fact.clone()) {
            return false;
        }

        self.facts.push(fact);
        self.facts.sort();
        true
    }
}

pub fn core_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            Atom::new("active", [Term::var("continuation")]),
            vec![
                Atom::new("continuation", [Term::var("continuation")]),
                Atom::new(
                    "continuation_status",
                    [Term::var("continuation"), Term::lit("open")],
                ),
            ],
        ),
        Rule::new(
            Atom::new("needs_review", [Term::var("continuation")]),
            vec![
                Atom::new("active", [Term::var("continuation")]),
                Atom::new("risk_level", [Term::var("continuation"), Term::lit("high")]),
            ],
        ),
        Rule::new(
            Atom::new(
                "risk_marker",
                [Term::var("continuation"), Term::lit("review")],
            ),
            vec![Atom::new("needs_review", [Term::var("continuation")])],
        ),
    ]
}

fn derive_rule(rule: &Rule, facts: &[RuleFact]) -> Vec<RuleFact> {
    let mut bindings = vec![Bindings::new()];

    for atom in &rule.body {
        bindings = match_atom(atom, facts, &bindings);
        if bindings.is_empty() {
            return Vec::new();
        }
    }

    bindings
        .iter()
        .filter_map(|binding| instantiate_head(&rule.head, binding))
        .collect()
}

fn match_atom(atom: &Atom, facts: &[RuleFact], bindings: &[Bindings]) -> Vec<Bindings> {
    let mut next = Vec::new();

    for binding in bindings {
        for fact in facts {
            if fact.predicate != atom.predicate || fact.args.len() != atom.terms.len() {
                continue;
            }

            if let Some(updated) = unify(atom, fact, binding) {
                next.push(updated);
            }
        }
    }

    next
}

fn unify(atom: &Atom, fact: &RuleFact, binding: &Bindings) -> Option<Bindings> {
    let mut updated = binding.clone();

    for (term, value) in atom.terms.iter().zip(&fact.args) {
        match term {
            Term::Lit(expected) if expected != value => return None,
            Term::Lit(_) => {}
            Term::Var(name) => match updated.get(name) {
                Some(bound) if bound != value => return None,
                Some(_) => {}
                None => {
                    updated.insert(name.clone(), value.clone());
                }
            },
        }
    }

    Some(updated)
}

fn instantiate_head(head: &Atom, binding: &Bindings) -> Option<RuleFact> {
    let mut args = Vec::with_capacity(head.terms.len());

    for term in &head.terms {
        match term {
            Term::Lit(value) => args.push(value.clone()),
            Term::Var(name) => args.push(binding.get(name)?.clone()),
        }
    }

    Some(RuleFact::new(head.predicate.clone(), args))
}
