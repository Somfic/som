use grammar::{Grammar, NonTerminal, Term};

use crate::{
    diagnostic::{Diagnostic, Error},
    scanner::token::Token,
};
use std::collections::HashSet;

pub mod grammar;

#[derive(Debug)]
pub struct Chart<'a> {
    pub states: Vec<Vec<EarleyItem<'a>>>,
}

impl Default for Chart<'_> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<'a> Chart<'a> {
    fn new(input_len: usize) -> Self {
        Chart {
            states: vec![vec![]; input_len + 1],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EarleyItem<'a> {
    pub head: NonTerminal,
    pub body: Vec<Term>,
    pub dot: usize,
    pub start: usize,
    pub tree: Vec<ParseNode<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseNode<'a> {
    Terminal(Token<'a>),
    NonTerminal(NonTerminal, Vec<ParseNode<'a>>),
}

impl<'a> std::fmt::Display for EarleyItem<'a> {
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

impl<'a> EarleyItem<'a> {
    pub fn new(head: NonTerminal, body: Vec<Term>, dot: usize, start: usize) -> Self {
        Self {
            head,
            body,
            dot,
            start,
            tree: Vec::new(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.dot >= self.body.len()
    }

    pub fn next(&self) -> Option<&Term> {
        self.body.get(self.dot)
    }
}

#[derive(Debug, Default)]
pub struct EarleyParser<'a> {
    grammar: Grammar,
    chart: Chart<'a>,
}

impl<'a> EarleyParser<'a> {
    /// Parses the given input tokens according to the grammar.
    /// Returns true if the input is accepted by the grammar, otherwise false.
    pub fn parse(mut self, tokens: &'a [Token]) -> Result<ParseNode<'a>, Vec<Diagnostic<'a>>> {
        let mut diagnostics = Vec::new();

        self.chart = Chart::new(tokens.len());

        // Initial state
        if let Some(start_rules) = self.grammar.get(&NonTerminal::Start) {
            for rule in start_rules {
                self.chart.states[0].push(EarleyItem::new(NonTerminal::Start, rule.clone(), 0, 0));
            }
        }

        for i in 0..=tokens.len() {
            let token = tokens.get(i);
            let mut j = 0;

            if self.chart.states[i].is_empty() {
                let expected_symbols: Vec<String> = self
                    .chart
                    .states
                    .get(i - 1)
                    .map(|state| {
                        state
                            .iter()
                            .filter_map(|item| item.next())
                            .filter_map(|term| match term {
                                Term::Terminal(token_type) => Some(token_type.to_string()),
                                _ => None,
                            })
                            .collect::<HashSet<String>>()
                    })
                    .iter()
                    .flat_map(|set| set.clone())
                    .collect::<Vec<_>>();

                let token = tokens.get(i).unwrap_or(tokens.last().unwrap());

                if !expected_symbols.is_empty() {
                    diagnostics.push(
                        Diagnostic::error("Syntax error").with_error(
                            Error::primary(
                                token.range.file_id,
                                i - 1,
                                0,
                                format!("Expected {}", expected_symbols.join(" or ")),
                            )
                            .transform_range(tokens),
                        ),
                    );
                }

                // TODO: enter panic mode
            }

            while j < self.chart.states[i].len() {
                let item = self.chart.states[i][j].clone();
                if let Some(next_symbol) = item.next() {
                    match next_symbol {
                        Term::NonTerminal(non_terminal) => {
                            self.predict(i, non_terminal);
                        }
                        Term::Terminal(token_type) => {
                            if i < tokens.len() && token_type == &token.unwrap().token_type {
                                self.scan(i, &item, token.unwrap());
                            }
                        }
                    }
                } else {
                    self.complete(i, &item);
                }
                j += 1;
            }
        }

        let matched = self.chart.states[tokens.len()].iter().find(|item| {
            item.head == NonTerminal::Start && item.dot == item.body.len() && item.start == 0
        });

        if let Some(item) = matched {
            Ok(ParseNode::NonTerminal(
                NonTerminal::Start,
                item.tree.clone(),
            ))
        } else {
            // Expected more input ...
            let expected_symbols: Vec<String> = self
                .chart
                .states
                .get(tokens.len())
                .map(|state| {
                    state
                        .iter()
                        .filter_map(|item| item.next())
                        .filter_map(|term| match term {
                            Term::Terminal(token_type) => Some(token_type.to_string()),
                            _ => None,
                        })
                        .collect::<HashSet<String>>()
                })
                .iter()
                .flat_map(|set| set.clone())
                .collect::<Vec<_>>();

            let token = tokens.last().unwrap();

            if !expected_symbols.is_empty() {
                diagnostics.push(
                    Diagnostic::error("Syntax error").with_error(
                        Error::primary(
                            token.range.file_id,
                            tokens.len(),
                            0,
                            format!("Expected {}", expected_symbols.join(" or ")),
                        )
                        .transform_range(tokens),
                    ),
                );
            }

            Err(diagnostics)
        }
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
        } else {
            panic!("No rules found for non-terminal: {}", non_terminal);
        }
    }

    /// Scans the next input token and advances the dot in the corresponding Earley item.
    fn scan(&mut self, position: usize, item: &EarleyItem<'a>, token: &Token<'a>) {
        let mut next_item = EarleyItem::<'a>::new(
            item.head.clone(),
            item.body.clone(),
            item.dot + 1,
            item.start,
        );

        next_item.tree.clone_from(&item.tree);
        next_item.tree.push(ParseNode::Terminal(token.clone()));

        if !self.chart.states[position + 1].contains(&next_item) {
            self.chart.states[position + 1].push(next_item);
        }
    }

    /// Completes a rule when the dot has reached the end of the right-hand side,
    /// and propagates this completion to other Earley items that were waiting for this rule.
    fn complete(&mut self, position: usize, item: &EarleyItem<'a>) {
        let start_state_set = self.chart.states[item.start].clone();
        for state in start_state_set {
            if let Some(Term::NonTerminal(non_terminal)) = state.next() {
                if non_terminal == &item.head {
                    let mut next_item = EarleyItem::new(
                        state.head.clone(),
                        state.body.clone(),
                        state.dot + 1,
                        state.start,
                    );
                    next_item.tree.clone_from(&state.tree);
                    next_item
                        .tree
                        .push(ParseNode::NonTerminal(item.head.clone(), item.tree.clone()));
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
    use crate::{files::Files, scanner::Scanner};

    #[test]
    pub fn test() {
        let mut files = Files::default();
        files.insert(
            "main",
            "
            enum test: red green blue;
    ",
        );

        let scanner = Scanner::new(&files);
        let tokens = match scanner.parse() {
            Ok(tokens) => tokens,
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    diagnostic.print(&files);
                }
                panic!("Failed to scan");
            }
        };

        let parser = super::EarleyParser::default();
        match parser.parse(&tokens) {
            Ok(tree) => {
                println!("{:#?}", tree);
            }
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    diagnostic.print(&files);
                }
            }
        }
    }
}
