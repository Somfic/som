use std::collections::HashMap;

use crate::scanner::token::TokenType;

#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Terminal(TokenType),
    NonTerminal(NonTerminal),
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::Terminal(token_type) => write!(f, "{}", token_type),
            Term::NonTerminal(non_terminal) => write!(f, "{}", non_terminal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NonTerminal {
    Start,
    RootItems,
    RootItem,
    EnumDeclaration,
    EnumItems,
    EnumItem,
}

impl std::fmt::Display for NonTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{:?}>", self)
    }
}

#[derive(Debug)]
pub struct Grammar {
    pub rules: HashMap<NonTerminal, Vec<Vec<Term>>>,
}

impl Default for Grammar {
    fn default() -> Self {
        let mut grammar = Grammar {
            rules: HashMap::new(),
        };

        // start -> root_items
        grammar.add_rule(
            NonTerminal::Start,
            vec![Term::NonTerminal(NonTerminal::RootItems)],
        );
        // root_items -> root_item root_items | root_item
        grammar.add_rules(
            NonTerminal::RootItems,
            vec![
                vec![
                    Term::NonTerminal(NonTerminal::RootItems),
                    Term::NonTerminal(NonTerminal::RootItem),
                ],
                vec![Term::NonTerminal(NonTerminal::RootItem)],
            ],
        );
        // root_item -> enum_declaration
        grammar.add_rule(
            NonTerminal::RootItem,
            vec![Term::NonTerminal(NonTerminal::EnumDeclaration)],
        );
        // enum_declaration -> <enum> <identifier> <colon> <identifier>? <semicolon>
        grammar.add_rule(
            NonTerminal::EnumDeclaration,
            vec![
                Term::Terminal(TokenType::Enum),
                Term::Terminal(TokenType::Identifier),
                Term::Terminal(TokenType::Colon),
                Term::NonTerminal(NonTerminal::EnumItems),
                Term::Terminal(TokenType::Semicolon),
            ],
        );
        // enum_items -> enum_item enum_items | enum_item
        grammar.add_rules(
            NonTerminal::EnumItems,
            vec![
                vec![
                    Term::NonTerminal(NonTerminal::EnumItem),
                    Term::NonTerminal(NonTerminal::EnumItems),
                ],
                vec![Term::NonTerminal(NonTerminal::EnumItem)],
            ],
        );
        // enum_item -> <identifier>
        grammar.add_rule(
            NonTerminal::EnumItem,
            vec![Term::Terminal(TokenType::Identifier)],
        );

        grammar
    }
}

impl Grammar {
    pub fn add_rule(&mut self, non_terminal: NonTerminal, rule: Vec<Term>) {
        self.rules.entry(non_terminal).or_default().push(rule);
    }

    pub fn add_rules(&mut self, non_terminal: NonTerminal, rules: Vec<Vec<Term>>) {
        self.rules.entry(non_terminal).or_default().extend(rules);
    }

    pub fn get(&self, start: &NonTerminal) -> Option<&Vec<Vec<Term>>> {
        self.rules.get(start)
    }
}
