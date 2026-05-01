use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleFact {
    pub predicate: String,
    pub args: Vec<String>,
}

#[derive(Debug, Default)]
pub struct RuleEngine {
    facts: Vec<RuleFact>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn assert_fact(&mut self, fact: RuleFact) {
        self.facts.push(fact);
    }

    pub fn facts(&self) -> &[RuleFact] {
        &self.facts
    }
}
