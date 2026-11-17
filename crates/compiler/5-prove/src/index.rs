//! Indexes inside what they index, and divisors not 0.

use som_ast::{Expr, Id};
use som_base::message::code;
use som_base::{Fix, Message, msg};

use crate::facts::{Compare, Cond, Term, Value};
use crate::{Place, ProveError, Prover, as_is};

/// The least and most something is known to be, where either is.
type Known = (Option<i128>, Option<i128>);

impl Prover<'_> {
    /// `object[index]`: an array's, inside its length; any other's, its `at`'s `@where`.
    /// `reading`: not assigned to, so `get` would do instead.
    pub(crate) fn check_index(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        index: Id<Expr>,
        reading: bool,
    ) {
        if let Some(func) = self.method(id) {
            if !(self.unproven(id, func, 1, Some(object), &[index])).is_empty() {
                self.out_of_bounds(id, object, index, reading);
            }
            return;
        }
        let Some(len) = self.array_len(object) else {
            return;
        };
        // A number written out is checked against a length written out by the type checker;
        // against `N`, here.
        if matches!(self.ast.exprs[index], Expr::Int(_)) && len.is_number() {
            return;
        }
        // Inside is from 0 up to the length: a signed index can be below 0, and `as usize`
        // would take it far past the end.
        let at = self.value_of(index, &as_is);
        let known = at.is_some_and(|(at, needs)| {
            self.all_hold(&needs)
                && self.facts.holds(&Cond::compare(
                    &at,
                    Compare::GreaterOrEqual,
                    &Value::number(0),
                ))
                && self.facts.holds(&Cond::compare(&at, Compare::Less, &len))
        });
        // `y * 10 + x`, of an array of a known length: where it's known to be from what's
        // known of `x` and `y`.
        let known = known
            || (len.is_number()
                && (self.range_of(index))
                    .is_some_and(|(least, most)| least >= 0 && most < len.offset));
        if !known {
            self.out_of_bounds(id, object, index, reading);
        }
    }

    /// `object[index]` not known to be inside it.
    fn out_of_bounds(&mut self, id: Id<Expr>, object: Id<Expr>, index: Id<Expr>, get: bool) {
        let object_text = self.ast.exprs.span(object).get_text();
        let index_text = self.ast.exprs.span(index).get_text();
        let len_text = format!("{object_text}.{}", som_lang::LEN);
        let known = self.known_about(object, index, &index_text, &len_text);
        // Known to be outside, not just not known to be inside.
        let never = self.index_outside(object, index);
        let written = matches!(self.ast.exprs[index], Expr::Int(_));
        // A signed index known not to be below 0 only needs the length checked.
        let signed = !written && !self.is_unsigned(index) && !self.at_least_zero(index);
        let check = match (written, signed) {
            (true, _) => format!("{len_text} > {index_text}"),
            (false, true) => format!("{index_text} >= 0 && {index_text} < {len_text}"),
            (false, false) => format!("{index_text} < {len_text}"),
        };
        let title = match written {
            true => msg![
                "check the length first: ",
                code(format!("if {check} {{ … }}"))
            ],
            false => msg!["check it first: ", code(format!("if {check} {{ … }}"))],
        };
        let fix = self.check_fix(&check, title);
        let get_fix = get.then(|| {
            let get = format!("{object_text}.get({index_text})");
            Fix::replace(
                msg!["use ", code(get.clone()), ", which gives an optional"],
                self.ast.exprs.span(id).clone(),
                get,
            )
        });
        self.errors.push(ProveError::MaybeOutOfBounds {
            never,
            index: index_text.to_string(),
            check,
            written,
            signed,
            len: len_text,
            known,
            span: self.ast.exprs.span(index).clone(),
            fix,
            get_fix,
        });
    }

    /// Whether index `index` of `object` is known to be at or past its end.
    fn index_outside(&mut self, object: Id<Expr>, index: Id<Expr>) -> bool {
        let Some(len) = self.len_of(object) else {
            return false;
        };
        let Some((at, _)) = self.value_of(index, &as_is) else {
            return false;
        };
        self.facts.refuted(&Cond::compare(&at, Compare::Less, &len))
    }

    /// What is known about index `index` of `object`, for when it isn't enough: how it
    /// compares with the length, or what each is known to be.
    fn known_about(
        &mut self,
        object: Id<Expr>,
        index: Id<Expr>,
        index_text: &str,
        len_text: &str,
    ) -> Message {
        let len = self.len_of(object);
        let at = self.value_of(index, &as_is).map(|(at, _)| at);
        let Some(len) = len else {
            return describe(len_text, (None, None));
        };
        let len_known = describe(len_text, self.known_range(&len));
        let Some(at) = at else {
            return len_known;
        };
        if at.is_number() {
            return len_known;
        }
        // `i <= xs.len`: how the index compares with the length, when it's no number.
        let at_range = self.known_range(&at);
        if at_range.1.is_none()
            && at.minus == Term::Zero
            && let Some(d) = self.facts.bound(&at.term, &len.term)
        {
            let above = d + at.offset - len.offset;
            let limit = match above {
                0 => len_text.to_string(),
                n if n > 0 => format!("{len_text} + {n}"),
                n => format!("{len_text} - {}", -n),
            };
            return msg![
                code(index_text.to_string()),
                " is only known to be at most ",
                code(limit)
            ];
        }
        msg![describe(index_text, at_range), ", and ", len_known]
    }

    /// The least and most `v` is known to be, for saying so.
    fn known_range(&self, v: &Value<Place>) -> Known {
        // Unsigned values are at least 0 anyway: that's no news.
        let least = (self.facts.bound(&v.minus, &v.term))
            .map(|d| v.offset - d)
            .filter(|&n| n > 0);
        let most = self.facts.bound(&v.term, &v.minus).map(|d| d + v.offset);
        (least, most)
    }

    /// The divisor of an integer `/` or `%`: known not to be 0, as dividing by it has no
    /// answer and nothing is checked as the program runs.
    pub(crate) fn check_divisor(&mut self, divisor: Id<Expr>) {
        if self.nonzero(divisor) {
            return;
        }
        let never = match self.value_of(divisor, &as_is) {
            Some((value, needs)) => self.facts.refuted(&not_zero(value, needs)),
            None => false,
        };
        let text = self.ast.exprs.span(divisor).get_text().to_string();
        let simple = text
            .chars()
            .all(|c| c.is_alphanumeric() || "_.".contains(c));
        let check = match simple {
            true => format!("{text} != 0"),
            false => format!("({text}) != 0"),
        };
        // `12 / 0`: no check would do.
        let written = matches!(self.ast.exprs[divisor], Expr::Int(_));
        let title = msg!["check it first: ", code(format!("if {check} {{ … }}"))];
        let fix = (!written).then(|| self.check_fix(&check, title)).flatten();
        self.errors.push(ProveError::MaybeZeroDivisor {
            written,
            divisor: text,
            check,
            never,
            span: self.ast.exprs.span(divisor).clone(),
            fix,
        });
    }

    /// Whether `expr` is known not to be 0. Of `a if c else b`, each where it's chosen: `a`
    /// where `c` holds, `b` where it doesn't.
    fn nonzero(&mut self, expr: Id<Expr>) -> bool {
        if let Expr::Conditional {
            condition,
            then,
            otherwise,
        } = self.ast.exprs[expr]
        {
            let mut both = true;
            for (branch, truth) in [(then, true), (otherwise, false)] {
                let mark = self.facts.mark();
                let cond = self.cond(condition, truth, &as_is);
                self.facts.assume(&cond);
                both &= self.nonzero(branch);
                self.facts.forget_since(mark);
            }
            return both;
        }
        // `(n % 4) + 1`: from what's known of what it's made of.
        if (self.range_of(expr)).is_some_and(|(least, most)| least > 0 || most < 0) {
            return true;
        }
        let Some((value, needs)) = self.value_of(expr, &as_is) else {
            return false;
        };
        self.facts.holds(&not_zero(value, needs))
    }

    /// Whether `expr` is known not to be below 0.
    fn at_least_zero(&mut self, expr: Id<Expr>) -> bool {
        if self.range_of(expr).is_some_and(|(least, _)| least >= 0) {
            return true;
        }
        self.known_value(expr).is_some_and(|value| {
            self.facts.holds(&Cond::compare(
                &value,
                Compare::GreaterOrEqual,
                &Value::number(0),
            ))
        })
    }
}

/// `value != 0`, where what it needs holds.
fn not_zero(value: Value<Place>, needs: Vec<Cond<Place>>) -> Cond<Place> {
    let nonzero = Cond::compare(&value, Compare::NotEqual, &Value::number(0));
    Cond::All(needs.into_iter().chain([nonzero]).collect())
}

/// What's known of `subject`, from the least and most it's known to be.
fn describe(subject: &str, (least, most): Known) -> Message {
    let subject = code(subject.to_string());
    match (least, most) {
        (Some(l), Some(m)) if l == m => msg![subject, " is ", l.to_string()],
        (Some(l), Some(m)) => msg![
            subject,
            " is only known to be from ",
            code(l.to_string()),
            " to ",
            code(m.to_string())
        ],
        (Some(l), None) => msg![
            subject,
            " is only known to be at least ",
            code(l.to_string())
        ],
        (None, Some(m)) => msg![
            subject,
            " is only known to be at most ",
            code(m.to_string())
        ],
        (None, None) => msg!["nothing is known about ", subject, " here"],
    }
}
