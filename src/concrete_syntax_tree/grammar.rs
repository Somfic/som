use std::collections::HashMap;

use crate::scanner::token::TokenType;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Terminal(TokenType),
    NonTerminal(NonTerminal),
    OneOrMore(NonTerminal),
    ZeroOrMore(NonTerminal),
    Optional(NonTerminal),
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Symbol::Terminal(token_type) => write!(f, "{}", token_type),
            Symbol::NonTerminal(non_terminal) => write!(f, "{}", non_terminal),
            Symbol::OneOrMore(non_terminal) => write!(f, "{}+", non_terminal),
            Symbol::ZeroOrMore(non_terminal) => write!(f, "{}*", non_terminal),
            Symbol::Optional(non_terminal) => write!(f, "{}?", non_terminal),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NonTerminal {
    Start,
    RootItem,
    EnumDeclaration,
    EnumItem,
}

impl std::fmt::Display for NonTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{:?}>", self)
    }
}

#[derive(Debug)]
pub struct Grammar {
    pub rules: HashMap<NonTerminal, Vec<Vec<Symbol>>>,
}

impl Default for Grammar {
    fn default() -> Self {
        let mut grammar = Grammar {
            rules: HashMap::new(),
        };

        // start -> root_item+
        grammar.add_rule(
            NonTerminal::Start,
            vec![Symbol::ZeroOrMore(NonTerminal::RootItem)],
        );

        // root_item -> enum_declaration
        grammar.add_rule(
            NonTerminal::RootItem,
            vec![Symbol::NonTerminal(NonTerminal::EnumDeclaration)],
        );
        // enum_declaration -> <enum> <identifier> <colon> enum_item+ <semicolon>
        grammar.add_rule(
            NonTerminal::EnumDeclaration,
            vec![
                Symbol::Terminal(TokenType::Enum),
                Symbol::Terminal(TokenType::Identifier),
                Symbol::Terminal(TokenType::Colon),
                Symbol::OneOrMore(NonTerminal::EnumItem),
                Symbol::Terminal(TokenType::Semicolon),
            ],
        );

        // enum_item -> <identifier>
        grammar.add_rule(
            NonTerminal::EnumItem,
            vec![Symbol::Terminal(TokenType::Identifier)],
        );

        grammar
    }
}

impl Grammar {
    pub fn add_rule(&mut self, non_terminal: NonTerminal, rule: Vec<Symbol>) {
        self.rules.entry(non_terminal).or_default().push(rule);
    }

    pub fn add_rules(&mut self, non_terminal: NonTerminal, rules: Vec<Vec<Symbol>>) {
        self.rules.entry(non_terminal).or_default().extend(rules);
    }

    pub fn get(&self, start: &NonTerminal) -> Option<&Vec<Vec<Symbol>>> {
        self.rules.get(start)
    }
}
