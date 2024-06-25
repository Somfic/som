use grammar::{Grammar, NonTerminal, Symbol};

use crate::{
    abstract_syntax_tree::AstractSyntax,
    diagnostic::{Diagnostic, Error, Range},
    scanner::token::Token,
};
use std::{collections::HashSet, fmt::Display};

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
    pub body: Vec<Symbol>,
    pub dot: usize,
    pub start: usize,
    pub tree: Vec<ConcreteSyntax<'a>>,
    ast_node: Option<AstractSyntax<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConcreteSyntax<'a> {
    Terminal(Token<'a>),
    NonTerminal(NonTerminal, Vec<ConcreteSyntax<'a>>),
}

impl<'a> Display for ConcreteSyntax<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConcreteSyntax::Terminal(token) => write!(f, "{}", token.token_type),
            ConcreteSyntax::NonTerminal(non_terminal, children) => {
                write!(f, "{}", non_terminal)?;
                if !children.is_empty() {
                    // And (n) children
                    write!(f, " ({})", children.len())?;
                }
                Ok(())
            }
        }
    }
}

impl<'a> ConcreteSyntax<'a> {
    pub fn range(&'a self) -> Range<'a> {
        match self {
            ConcreteSyntax::Terminal(token) => token.range.clone(),
            ConcreteSyntax::NonTerminal(_, children) => {
                let start = children.first().unwrap().range().position;
                let end = children.last().unwrap().range();
                let end = end.position + end.length;
                Range {
                    file_id: children.first().unwrap().range().file_id,
                    position: start,
                    length: end - start,
                }
            }
        }
    }
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
    pub fn new(head: NonTerminal, body: Vec<Symbol>, dot: usize, start: usize) -> Self {
        Self {
            head,
            body,
            dot,
            start,
            tree: Vec::new(),
            ast_node: None,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.dot >= self.body.len()
    }

    pub fn next(&self) -> Option<&Symbol> {
        self.body.get(self.dot)
    }
}

#[derive(Debug, Default)]
pub struct EarleyParser<'a> {
    grammar: Grammar,
    chart: Chart<'a>,
    diagnostics: Vec<Diagnostic<'a>>,
}

impl<'a> EarleyParser<'a> {
    /// Parses the given input tokens according to the grammar.
    /// Returns true if the input is accepted by the grammar, otherwise false.
    pub fn parse(mut self, tokens: &'a [Token]) -> Result<ConcreteSyntax<'a>, Vec<Diagnostic<'a>>> {
        self.diagnostics = Vec::new();
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
                self.add_diagnostic(tokens, i - 1);
                // TODO: enter panic mode
            }

            while j < self.chart.states[i].len() {
                let item = self.chart.states[i][j].clone();
                if let Some(next_symbol) = item.next() {
                    match next_symbol {
                        Symbol::NonTerminal(non_terminal) => {
                            self.predict(i, non_terminal);
                        }
                        Symbol::Terminal(token_type) => {
                            if i < tokens.len() && token_type == &token.unwrap().token_type {
                                self.scan(i, &item, token.unwrap());
                            }
                        }
                        Symbol::OneOrMore(non_terminal) => {
                            self.handle_one_or_more(i, &item, non_terminal);
                        }
                        Symbol::ZeroOrMore(non_terminal) => {
                            self.handle_zero_or_more(i, &item, non_terminal);
                        }
                        Symbol::Optional(non_terminal) => {
                            self.handle_optional(i, &item, non_terminal);
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
            Ok(ConcreteSyntax::NonTerminal(
                NonTerminal::Start,
                item.tree.clone(),
            ))
        } else {
            self.add_diagnostic(tokens, tokens.len());
            Err(self.diagnostics)
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
        next_item.tree.push(ConcreteSyntax::Terminal(token.clone()));

        if !self.chart.states[position + 1].contains(&next_item) {
            self.chart.states[position + 1].push(next_item);
        }
    }

    /// Completes a rule when the dot has reached the end of the right-hand side,
    /// and propagates this completion to other Earley items that were waiting for this rule.
    fn complete(&mut self, position: usize, item: &EarleyItem<'a>) {
        let start_state_set = self.chart.states[item.start].clone();
        for state in start_state_set {
            if let Some(Symbol::NonTerminal(non_terminal)) = state.next() {
                if non_terminal == &item.head {
                    let mut next_item = EarleyItem::new(
                        state.head.clone(),
                        state.body.clone(),
                        state.dot + 1,
                        state.start,
                    );
                    next_item.tree.clone_from(&state.tree);
                    next_item.tree.push(ConcreteSyntax::NonTerminal(
                        item.head.clone(),
                        item.tree.clone(),
                    ));
                    if !self.chart.states[position].contains(&next_item) {
                        self.chart.states[position].push(next_item);
                    }
                }
            }
        }
    }

    fn add_diagnostic(&mut self, tokens: &'a [Token], index: usize) {
        let expected_symbols: Vec<String> = self
            .chart
            .states
            .get(index)
            .map(|state| {
                state
                    .iter()
                    .filter_map(|item| item.next())
                    .filter_map(|term| match term {
                        Symbol::Terminal(token_type) => Some(token_type.to_string()),
                        _ => None,
                    })
                    .collect::<HashSet<String>>()
            })
            .iter()
            .flat_map(|set| set.clone())
            .collect::<Vec<_>>();

        let token = tokens.get(index).unwrap_or(tokens.last().unwrap());

        if !expected_symbols.is_empty() {
            self.diagnostics.push(
                Diagnostic::error("Syntax error").with_error(
                    Error::primary(
                        token.range.file_id,
                        index,
                        0,
                        format!("Expected {}", expected_symbols.join(" or ")),
                    )
                    .transform_range(tokens),
                ),
            );
        }
    }
}
