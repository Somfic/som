//! What's known to hold at a point in a program, and whether a condition follows from it.
//! It knows nothing of the language: what's compared is places of any type `P`, which the
//! pass makes from expressions (a variable, `xs.len`), and numbers.
//!
//! A comparison is kept as `a - b <= c`. `a < b` is `a - b <= -1`; `a == b` is two of them.
//! Whether `x - y <= d` follows is whether the shortest chain of what's known from `x` to `y`
//! adds up to at most `d`: `i - n <= -1` and `n - xs.len <= 0` give `i - xs.len <= -1`.
//!
//! Facts are learned in scopes: `mark` where one starts, `forget_since` where it ends. One
//! about a place that changes is forgotten with `forget`.

/// A value comparisons are about.
#[derive(Clone, PartialEq, Debug)]
pub enum Term<P> {
    /// The number 0, which numbers are offsets from: `3` is `Zero + 3`.
    Zero,
    /// A place whose value isn't known: `i`, `xs.len`.
    Place(P),
}

/// A term and a number added to it: `i + 1`, `xs.len - 1`, `3`.
#[derive(Clone, PartialEq, Debug)]
pub struct Value<P> {
    pub term: Term<P>,
    pub offset: i128,
}

impl<P> Value<P> {
    pub fn number(n: i128) -> Self {
        Value {
            term: Term::Zero,
            offset: n,
        }
    }

    pub fn place(place: P) -> Self {
        Value {
            term: Term::Place(place),
            offset: 0,
        }
    }
}

/// A condition, as far as it's followed.
#[derive(Clone, Debug)]
pub enum Cond<P> {
    /// Each of them. With none, what's always true.
    All(Vec<Cond<P>>),
    /// One of them, not known which. With none, what's never true.
    Any(Vec<Cond<P>>),
    /// `a - b <= c`.
    Le(Term<P>, Term<P>, i128),
    /// A `bool` place that's this.
    Is(P, bool),
    /// Something not followed: never known to hold.
    Unknown,
}

/// How two values are compared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Compare {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
}

impl Compare {
    /// What holds when this doesn't: `>=` for `<`.
    pub fn opposite(self) -> Compare {
        match self {
            Compare::Less => Compare::GreaterOrEqual,
            Compare::LessOrEqual => Compare::Greater,
            Compare::Greater => Compare::LessOrEqual,
            Compare::GreaterOrEqual => Compare::Less,
            Compare::Equal => Compare::NotEqual,
            Compare::NotEqual => Compare::Equal,
        }
    }
}

impl<P: Clone> Cond<P> {
    /// `x op y`.
    pub fn compare(x: &Value<P>, op: Compare, y: &Value<P>) -> Cond<P> {
        // `x <= y` is `x.term - y.term <= y.offset - x.offset`; `<` one less.
        let le = |a: &Value<P>, b: &Value<P>, strict: bool| {
            Cond::Le(
                a.term.clone(),
                b.term.clone(),
                b.offset - a.offset - strict as i128,
            )
        };
        match op {
            Compare::Less => le(x, y, true),
            Compare::LessOrEqual => le(x, y, false),
            Compare::Greater => le(y, x, true),
            Compare::GreaterOrEqual => le(y, x, false),
            Compare::Equal => Cond::All(vec![le(x, y, false), le(y, x, false)]),
            Compare::NotEqual => Cond::Any(vec![le(x, y, true), le(y, x, true)]),
        }
    }

    /// Whether some part of it isn't followed.
    pub fn has_unknown(&self) -> bool {
        match self {
            Cond::All(all) | Cond::Any(all) => all.iter().any(|c| c.has_unknown()),
            Cond::Unknown => true,
            Cond::Le(..) | Cond::Is(..) => false,
        }
    }

    pub fn always() -> Cond<P> {
        Cond::All(Vec::new())
    }

    pub fn never() -> Cond<P> {
        Cond::Any(Vec::new())
    }
}

#[derive(Clone, Debug)]
struct Fact<P> {
    /// When it was learned, for `forget_since`.
    id: usize,
    kind: FactKind<P>,
}

#[derive(Clone, Debug)]
enum FactKind<P> {
    Le(Term<P>, Term<P>, i128),
    Is(P, bool),
}

impl<P> FactKind<P> {
    fn places(&self) -> Vec<&P> {
        let mut places = Vec::new();
        match self {
            FactKind::Le(a, b, _) => {
                for term in [a, b] {
                    if let Term::Place(p) = term {
                        places.push(p);
                    }
                }
            }
            FactKind::Is(p, _) => places.push(p),
        }
        places
    }
}

/// What's known, at the point being checked.
#[derive(Clone, Debug)]
pub struct Facts<P> {
    facts: Vec<Fact<P>>,
    learned: usize,
    /// Places never below 0: unsigned integers.
    unsigned: Vec<P>,
}

impl<P> Default for Facts<P> {
    fn default() -> Self {
        Facts {
            facts: Vec::new(),
            learned: 0,
            unsigned: Vec::new(),
        }
    }
}

impl<P: Clone + PartialEq> Facts<P> {
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Where a scope starts: what's learned after it is forgotten by `forget_since`.
    pub fn mark(&self) -> usize {
        self.learned
    }

    /// Ends what was learned since `mark`, as the scope it holds in ends.
    pub fn forget_since(&mut self, mark: usize) {
        self.facts.retain(|f| f.id < mark);
    }

    /// Ends what's known about the places `changed` says could have changed.
    pub fn forget(&mut self, changed: impl Fn(&P) -> bool) {
        self.facts
            .retain(|f| f.kind.places().into_iter().all(|p| !changed(p)));
    }

    /// `place` is never below 0, whatever's learned or forgotten.
    pub fn unsigned(&mut self, place: P) {
        if !self.unsigned.contains(&place) {
            self.unsigned.push(place);
        }
    }

    /// Learns that `cond` holds, until its scope ends or what it's about changes. Of an
    /// `Any`, nothing is learned: which one holds isn't known.
    pub fn assume(&mut self, cond: &Cond<P>) {
        match cond {
            Cond::All(all) => all.iter().for_each(|c| self.assume(c)),
            Cond::Le(a, b, c) => self.learn(FactKind::Le(a.clone(), b.clone(), *c)),
            Cond::Is(p, value) => self.learn(FactKind::Is(p.clone(), *value)),
            // Of several, the one that doesn't contradict what's known: `len != 0` of an
            // unsigned `len` is `len > 0`, as `len < 0` can't be.
            Cond::Any(any) => {
                let possible: Vec<&Cond<P>> = any.iter().filter(|c| !self.refuted(c)).collect();
                if let [only] = possible[..] {
                    self.assume(only);
                }
            }
            Cond::Unknown => {}
        }
    }

    /// Whether `cond` is known not to hold.
    pub fn refuted(&self, cond: &Cond<P>) -> bool {
        match cond {
            // `a - b <= c` fails where `b - a <= -c - 1` holds.
            Cond::Le(a, b, c) => self.holds(&Cond::Le(b.clone(), a.clone(), -c - 1)),
            Cond::Is(p, value) => self.holds(&Cond::Is(p.clone(), !value)),
            Cond::All(all) => all.iter().any(|c| self.refuted(c)),
            Cond::Any(any) => any.iter().all(|c| self.refuted(c)),
            Cond::Unknown => false,
        }
    }

    fn learn(&mut self, kind: FactKind<P>) {
        self.facts.push(Fact {
            id: self.learned,
            kind,
        });
        self.learned += 1;
    }

    /// Whether `cond` is known to hold.
    pub fn holds(&self, cond: &Cond<P>) -> bool {
        match cond {
            Cond::All(all) => all.iter().all(|c| self.holds(c)),
            Cond::Any(any) => any.iter().any(|c| self.holds(c)),
            Cond::Le(a, b, c) => self.least(a, b).is_none_or(|d| d <= *c),
            Cond::Is(p, value) => (self.facts.iter())
                .any(|f| matches!(&f.kind, FactKind::Is(q, v) if q == p && v == value)),
            Cond::Unknown => false,
        }
    }

    /// The least `d` known with `a - b <= d`, if one is: what's known of how far `a` can be
    /// above `b`. With `b` zero, the most `a` can be; with `a` zero, minus the least `b` can be.
    pub fn bound(&self, a: &Term<P>, b: &Term<P>) -> Option<i128> {
        self.least(a, b).filter(|&d| d != i128::MAX)
    }

    /// The least `d` known with `a - b <= d`, `i128::MAX` if there's none. `None` if what's
    /// known contradicts itself, which only happens in code that can't run: anything holds.
    fn least(&self, a: &Term<P>, b: &Term<P>) -> Option<i128> {
        if a == b {
            return Some(0);
        }
        // Terms by index: `a` first, `b` second, then 0, then the rest.
        let mut terms: Vec<&Term<P>> = vec![a, b, &Term::Zero];
        let index = |terms: &mut Vec<&Term<P>>, t: &Term<P>| -> usize {
            if let Some(i) = terms.iter().position(|n| *n == t) {
                return i;
            }
            terms.len()
        };
        let mut edges = Vec::new();
        for fact in &self.facts {
            if let FactKind::Le(x, y, c) = &fact.kind {
                let i = index(&mut terms, x);
                if i == terms.len() {
                    terms.push(x);
                }
                let j = index(&mut terms, y);
                if j == terms.len() {
                    terms.push(y);
                }
                edges.push((i, j, *c));
            }
        }
        // What's never below 0: `0 - t <= 0`.
        let zero = terms.iter().position(|t| **t == Term::Zero).unwrap_or(2);
        for (i, t) in terms.iter().enumerate() {
            let at_least_zero = match t {
                Term::Place(p) => self.unsigned.contains(p),
                Term::Zero => false,
            };
            if at_least_zero {
                edges.push((zero, i, 0));
            }
        }
        // Shortest chains from `a`, by Bellman-Ford: still shortening after a round per
        // term is a chain that goes round and adds up to less than 0, a contradiction.
        const FAR: i128 = i128::MAX / 4;
        let mut dist = vec![FAR; terms.len()];
        dist[0] = 0;
        for round in 0..=terms.len() {
            let mut shorter = false;
            for &(x, y, c) in &edges {
                if dist[x] < FAR && dist[x] + c < dist[y] {
                    dist[y] = dist[x] + c;
                    shorter = true;
                }
            }
            if !shorter {
                break;
            }
            if round == terms.len() {
                return None;
            }
        }
        Some(if dist[1] >= FAR { i128::MAX } else { dist[1] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(p: &'static str) -> Value<&'static str> {
        Value::place(p)
    }

    #[test]
    fn follows_a_chain() {
        let mut facts = Facts::default();
        facts.assume(&Cond::compare(&v("i"), Compare::Less, &v("n")));
        facts.assume(&Cond::compare(&v("n"), Compare::LessOrEqual, &v("len")));
        assert!(facts.holds(&Cond::compare(&v("i"), Compare::Less, &v("len"))));
        assert!(!facts.holds(&Cond::compare(&v("len"), Compare::Less, &v("i"))));
    }

    #[test]
    fn numbers_against_a_length() {
        let mut facts = Facts::default();
        facts.unsigned("len");
        // `len > 2` gives `1 < len`, not `3 < len`.
        facts.assume(&Cond::compare(&v("len"), Compare::Greater, &Value::number(2)));
        assert!(facts.holds(&Cond::compare(&Value::number(1), Compare::Less, &v("len"))));
        assert!(facts.holds(&Cond::compare(&Value::number(2), Compare::Less, &v("len"))));
        assert!(!facts.holds(&Cond::compare(&Value::number(3), Compare::Less, &v("len"))));
    }

    #[test]
    fn scopes_and_changes() {
        let mut facts = Facts::default();
        let mark = facts.mark();
        facts.assume(&Cond::compare(&v("i"), Compare::Less, &v("len")));
        let goal = Cond::compare(&v("i"), Compare::Less, &v("len"));
        assert!(facts.holds(&goal));
        facts.forget(|p| *p == "len");
        assert!(!facts.holds(&goal));
        facts.assume(&goal);
        facts.forget_since(mark);
        assert!(!facts.holds(&goal));
    }

    #[test]
    fn not_zero_of_unsigned_is_above_it() {
        let mut facts = Facts::default();
        facts.unsigned("len");
        facts.assume(&Cond::compare(&v("len"), Compare::NotEqual, &Value::number(0)));
        assert!(facts.holds(&Cond::compare(&v("len"), Compare::GreaterOrEqual, &Value::number(1))));
    }

    #[test]
    fn unsigned_is_at_least_zero() {
        let mut facts = Facts::default();
        facts.unsigned("i");
        facts.assume(&Cond::compare(&v("len"), Compare::Greater, &Value::number(0)));
        assert!(facts.holds(&Cond::compare(&Value::number(0), Compare::LessOrEqual, &v("i"))));
        assert!(facts.holds(&Cond::compare(&Value::number(0), Compare::Less, &v("len"))));
    }
}
