use std::{collections::HashMap, os::macos::raw::stat};

use crate::scanner::lexeme::{self, Lexeme, TokenType};

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

// 1 + 1;
// 2 + (2 + 2);
// START -> STATEMENTS
// STATEMENTS -> STATEMENT STATEMENTS | STATEMENT
// STATEMENT -> EXPRESSION SEMICOLON
// EXPRESSION -> EXPRESSION PLUS EXPRESSION
// EXPRESSION -> INTEGER

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NonTerminal {
    Start,
    Statements,
    Statement,
    Expression,
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

impl Grammar {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }
}

impl Grammar {
    pub fn add_rule(&mut self, non_terminal: NonTerminal, rule: Vec<Term>) {
        self.rules
            .entry(non_terminal)
            .or_insert_with(Vec::new)
            .push(rule);
    }

    pub fn add_rules(&mut self, non_terminal: NonTerminal, rules: Vec<Vec<Term>>) {
        self.rules
            .entry(non_terminal)
            .or_insert_with(Vec::new)
            .extend(rules);
    }

    fn get(&self, start: &NonTerminal) -> Option<&Vec<Vec<Term>>> {
        self.rules.get(start)
    }
}

#[derive(Debug)]
pub struct Chart {
    pub states: Vec<Vec<EarleyItem>>,
}

impl Chart {
    fn new(input_len: usize) -> Self {
        Chart {
            states: vec![vec![]; input_len + 1],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EarleyItem {
    pub head: NonTerminal,
    pub body: Vec<Term>,
    pub dot: usize,
    pub start: usize,
}

impl std::fmt::Display for EarleyItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> ", self.head)?;
        for (i, term) in self.body.iter().enumerate() {
            if i == self.dot {
                write!(f, "• ")?;
            }
            write!(f, "{} ", term)?;
        }
        if self.dot == self.body.len() {
            write!(f, "•")?;
        }
        write!(f, " [{}]", self.start)
    }
}

impl EarleyItem {
    pub fn new(head: NonTerminal, body: Vec<Term>, dot: usize, start: usize) -> Self {
        Self {
            head,
            body,
            dot,
            start,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.dot >= self.body.len()
    }

    pub fn next(&self) -> Option<&Term> {
        self.body.get(self.dot)
    }
}

#[derive(Debug)]
pub struct EarleyParser {
    grammar: Grammar,
    chart: Chart,
}

impl EarleyParser {
    pub fn new(grammar: Grammar) -> Self {
        EarleyParser {
            grammar,
            chart: Chart::new(0),
        }
    }

    /// Parses the given input tokens according to the grammar.
    /// Returns true if the input is accepted by the grammar, otherwise false.
    pub fn parse(&mut self, input: Vec<TokenType>) -> bool {
        self.chart = Chart::new(input.len());

        // Initial state
        if let Some(start_rules) = self.grammar.get(&NonTerminal::Start) {
            for rule in start_rules {
                self.chart.states[0].push(EarleyItem::new(NonTerminal::Start, rule.clone(), 0, 0));
            }
        }

        for i in 0..=input.len() {
            let mut j = 0;

            if self.chart.states[i].is_empty() {
                // Expected another input ...
                let terminal_symbols = self
                    .chart
                    .states
                    .get(i - 1)
                    .map(|state| {
                        state
                            .iter()
                            .filter_map(|item| item.next())
                            .filter_map(|term| match term {
                                Term::Terminal(token_type) => Some(token_type),
                                _ => None,
                            })
                            .collect::<Vec<&TokenType>>()
                    })
                    .unwrap_or_default();

                println!("Terminal symbols: {:?}", terminal_symbols);
            }

            while j < self.chart.states[i].len() {
                let item = self.chart.states[i][j].clone();
                if let Some(next_symbol) = item.next() {
                    match next_symbol {
                        Term::NonTerminal(non_terminal) => {
                            self.predict(i, non_terminal);
                        }
                        Term::Terminal(token_type) => {
                            if i < input.len() && token_type == &input[i] {
                                self.scan(i, &item);
                            }
                        }
                    }
                } else {
                    self.complete(i, &item);
                }
                j += 1;
            }
        }

        println!("States: {}", input.len());
        let states = self.chart.states.iter().enumerate().map(|(i, state)| {
            format!(
                "State {}: {}",
                i,
                state
                    .iter()
                    .map(|item| format!("{}", item))
                    .collect::<Vec<String>>()
                    .join("\n         ")
            )
        });

        for state in states {
            println!("{}", state);
        }

        let matched = self.chart.states[input.len()].iter().any(|item| {
            item.head == NonTerminal::Start && item.dot == item.body.len() && item.start == 0
        });

        if !matched {
            // Expected more input ...
            let terminal_symbols = self
                .chart
                .states
                .get(input.len())
                .map(|state| {
                    state
                        .iter()
                        .filter_map(|item| item.next())
                        .filter_map(|term| match term {
                            Term::Terminal(token_type) => Some(token_type),
                            _ => None,
                        })
                        .collect::<Vec<&TokenType>>()
                })
                .unwrap_or_default();

            println!("Terminal symbols: {:?}", terminal_symbols);
        }

        matched
    }

    /// Predicts the possible expansions of a non-terminal symbol at a given position in the input.
    fn predict(&mut self, position: usize, non_terminal: &NonTerminal) {
        if let Some(rules) = self.grammar.rules.get(non_terminal) {
            for rule in rules {
                let item = EarleyItem::new(non_terminal.clone(), rule.clone(), 0, position);

                if !self.chart.states[position].contains(&item) {
                    self.chart.states[position].push(item);
                }
            }
        }
    }

    /// Scans the next input token and advances the dot in the corresponding Earley item.
    fn scan(&mut self, position: usize, item: &EarleyItem) {
        let next_item = EarleyItem::new(
            item.head.clone(),
            item.body.clone(),
            item.dot + 1,
            item.start,
        );
        if !self.chart.states[position + 1].contains(&next_item) {
            self.chart.states[position + 1].push(next_item);
        }
    }

    /// Completes a rule when the dot has reached the end of the right-hand side,
    /// and propagates this completion to other Earley items that were waiting for this rule.
    fn complete(&mut self, position: usize, item: &EarleyItem) {
        let start_state_set = self.chart.states[item.start].clone();
        for state in start_state_set {
            if let Some(Term::NonTerminal(non_terminal)) = state.next() {
                if non_terminal == &item.head {
                    let next_item = EarleyItem::new(
                        state.head.clone(),
                        state.body.clone(),
                        state.dot + 1,
                        state.start,
                    );
                    if !self.chart.states[position].contains(&next_item) {
                        self.chart.states[position].push(next_item);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        files::Files,
        scanner::{lexeme::TokenType, Scanner},
    };

    use super::{Grammar, NonTerminal, Term};

    #[test]
    pub fn test() {
        let mut grammar = Grammar::new();

        // 1 + 1;
        // 2 + (2 + 2);
        // START -> STATEMENTS
        // STATEMENTS -> STATEMENT STATEMENTS | STATEMENT
        // STATEMENT -> EXPRESSION SEMICOLON
        // EXPRESSION -> EXPRESSION PLUS EXPRESSION
        // EXPRESSION -> INTEGER
        grammar.add_rule(
            NonTerminal::Start,
            vec![Term::NonTerminal(NonTerminal::Statements)],
        );

        grammar.add_rules(
            NonTerminal::Statements,
            vec![
                vec![
                    Term::NonTerminal(NonTerminal::Statement),
                    Term::NonTerminal(NonTerminal::Statements),
                ],
                vec![Term::NonTerminal(NonTerminal::Statement)],
            ],
        );

        grammar.add_rule(
            NonTerminal::Statement,
            vec![
                Term::NonTerminal(NonTerminal::Expression),
                Term::Terminal(TokenType::Semicolon),
            ],
        );

        grammar.add_rule(
            NonTerminal::Expression,
            vec![
                Term::NonTerminal(NonTerminal::Expression),
                Term::Terminal(TokenType::Plus),
                Term::NonTerminal(NonTerminal::Expression),
            ],
        );

        grammar.add_rules(
            NonTerminal::Expression,
            vec![
                vec![Term::Terminal(TokenType::Integer)],
                vec![Term::Terminal(TokenType::Decimal)],
            ],
        );

        let mut parser = super::EarleyParser::new(grammar);

        let mut files = Files::default();
        files.insert(
            "main",
            "
            12 + 12;
            12 + 1 + 12;
            ;
    ",
        );

        let scanner = Scanner::new(&files);
        let lexemes = scanner.parse().unwrap();
        let token_types = lexemes
            .iter()
            .map(|l| l.token_type.clone())
            .collect::<Vec<TokenType>>();

        assert!(parser.parse(token_types));
    }
}
