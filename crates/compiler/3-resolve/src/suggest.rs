//! What's said when a name isn't found: a close match, a `use`, or som's way of writing what
//! another language's name was for.

use som_ast::{Expr, Id, Ident, Path};
use som_base::diagnostics::{closest_match, snake_case};
use som_base::foreign;
use som_base::message::{self, Message, code};
use som_base::{Fix, Span, msg};

use crate::names::DefKind;
use crate::{Expect, ResolveError, Resolver};

const PRINTLN: &str = "println";

type Hint = (Message, Vec<Fix>);

impl Resolver<'_> {
    pub(crate) fn not_found(&self, path: &Path, expect: Expect) -> ResolveError {
        let name = path.to_string();
        let span = path.span();
        let (help, fixes) = self.suggest(path, expect).unwrap_or_default();
        let help = Some(help).filter(|h| !h.is_empty());
        match expect {
            Expect::Value => ResolveError::UnboundVariable {
                name,
                span,
                help,
                fixes,
            },
            Expect::Func => ResolveError::UnknownFunction {
                name,
                span,
                help,
                fixes,
            },
            Expect::Struct => ResolveError::UnknownStruct {
                name,
                span,
                help,
                fixes,
            },
            Expect::Type => ResolveError::UnknownType {
                name,
                span,
                help,
                fixes,
            },
            Expect::Trait => ResolveError::UnknownTrait {
                name,
                span,
                help,
                fixes,
            },
        }
    }

    fn suggest(&self, path: &Path, expect: Expect) -> Option<Hint> {
        if path.segments.len() > 1 {
            return self.suggest_qualified(path, expect);
        }
        let span = path.span();
        let name = &*path.name().name;
        let change_to = |text: String| vec![Fix::change_to(span.clone(), text)];
        let foreign = match expect {
            Expect::Value => self.other_language_hint(name, &change_to),
            Expect::Type => foreign_type_hint(name, &change_to),
            _ => None,
        };
        foreign
            .or_else(|| self.suggest_visible(name, expect, &change_to))
            .or_else(|| self.suggest_import(name, &span, expect, &change_to))
    }

    /// A close match among what the qualifier names: a module's items, or a struct's variants
    /// and methods.
    fn suggest_qualified(&self, path: &Path, expect: Expect) -> Option<Hint> {
        let name = &*path.name().name;
        let qualifier: Vec<&str> = path.qualifier().iter().map(|s| &*s.name).collect();
        let qualifier = qualifier.join("::");
        let candidates: Vec<String> = if let Some(&m) = self.modules.by_name.get(qualifier.as_str())
        {
            (self.modules.items[m].iter())
                .filter(|&(_, &d)| self.fits(d, expect))
                .map(|(n, _)| n.to_string())
                .collect()
        } else if let Some(owner) = self.lookup(&path.segments[0].name, Expect::Struct)
            && path.segments.len() == 2
        {
            let variants = match self.names.def(owner).kind {
                DefKind::Struct(s) => self.names.variants(s).to_vec(),
                _ => Vec::new(),
            };
            (variants.iter())
                .map(|&v| self.names.def(v).name.to_string())
                .chain((self.names.methods_of(owner).into_iter()).map(|(n, _)| n.to_string()))
                .collect()
        } else {
            return None;
        };
        if let Some(m) = best_match(name, &candidates) {
            let text = format!("{qualifier}::{m}");
            let fixes = vec![Fix::change_to(path.span(), text.clone())];
            return Some((msg!["did you mean ", code(&text), "?"], fixes));
        }
        self.constructors_hint(path, &qualifier, expect)
    }

    /// A close match among the names visible here.
    fn suggest_visible(
        &self,
        name: &str,
        expect: Expect,
        change_to: &dyn Fn(String) -> Vec<Fix>,
    ) -> Option<Hint> {
        let mut visible: Vec<String> = std::iter::once(self.cx.module)
            .chain(self.modules.imports[self.cx.module].iter().copied())
            .flat_map(|m| self.modules.items[m].iter())
            .filter(|&(_, &def)| self.fits(def, expect))
            .map(|(name, _)| name.to_string())
            .collect();
        if expect == Expect::Value {
            visible.extend(
                (self.cx.scopes.iter())
                    .flat_map(|s| s.keys())
                    .map(|n| n.to_string()),
            );
        }
        visible.sort();
        visible.dedup();
        let m = best_match(name, &visible)?;
        Some((
            msg!["did you mean ", expect.name(m), "?"],
            change_to(m.to_string()),
        ))
    }

    /// Defined in a loaded module that isn't in scope here, maybe spelled in snake_case.
    fn suggest_import(
        &self,
        name: &str,
        span: &Span,
        expect: Expect,
        change_to: &dyn Fn(String) -> Vec<Fix>,
    ) -> Option<Hint> {
        let snake = snake_case(name);
        let imports = &self.modules.imports[self.cx.module];
        let (module, found) = (self.ast.modules.iter().zip(&self.modules.items).enumerate())
            .filter(|&(index, (module, _))| {
                !module.name.is_empty() && index != self.cx.module && !imports.contains(&index)
            })
            .find_map(|(_, (module, items))| {
                [name, snake.as_str()]
                    .into_iter()
                    .find(|n| items.get(n).is_some_and(|&d| self.fits(d, expect)))
                    .map(|found| (module, found))
            })?;
        let m = &module.name;
        let (qualified, import) = (format!("{m}::{found}"), format!("use {m};"));
        let help = msg![
            expect.name(found),
            " is defined in module ",
            message::module(m),
            " but is not accessible from here. try ",
            code(&qualified),
            " or ",
            code(&import),
        ];
        let mut fixes = change_to(qualified);
        fixes.push(Fix::add_line(
            msg!["add ", code(&import)],
            span.source.clone(),
            &import,
        ));
        Some((help, fixes))
    }

    /// For `List::new` where `List` has no `new`: the functions that make one, those not
    /// taking `self`. Only for names other languages give constructors.
    fn constructors_hint(&self, path: &Path, qualifier: &str, expect: Expect) -> Option<Hint> {
        let name = &*path.name().name;
        if expect != Expect::Func
            || path.segments.len() != 2
            || !foreign::CONSTRUCTOR.contains(&name)
        {
            return None;
        }
        let owner = self.lookup(&path.segments[0].name, Expect::Struct)?;
        // With whether they take nothing at all.
        let constructors: Vec<(String, bool)> = (self.names.methods_of(owner).into_iter())
            .filter_map(|(n, def)| match self.names.def(def).kind {
                DefKind::Func(f) => {
                    let func = &self.ast.funcs[f];
                    let takes_self = (func.params.first())
                        .is_some_and(|p| &*p.name.name == som_lang::SELF_VALUE);
                    (!func.prop && !takes_self)
                        .then(|| (format!("{qualifier}::{n}"), func.params.is_empty()))
                }
                _ => None,
            })
            .collect();
        if constructors.is_empty() {
            return None;
        }
        let mut help = msg![
            code(qualifier),
            " has no ",
            code(name),
            "; to make one, use "
        ];
        for (i, (c, _)) in constructors.iter().enumerate() {
            if i > 0 {
                help.push(if i + 1 == constructors.len() {
                    " or "
                } else {
                    ", "
                });
            }
            help.push(code(c));
        }
        // Only those taking nothing can stand in as they are: `List::new()` → `List::empty()`.
        let fixes = (constructors.iter())
            .filter(|(_, nothing)| *nothing)
            .map(|(c, _)| Fix::change_to(path.span(), c.clone()))
            .collect();
        Some((help, fixes))
    }

    /// A value named as another language names it: `null` for `none`, `this` for `self`.
    fn other_language_hint(
        &self,
        name: &str,
        change_to: &dyn Fn(String) -> Vec<Fix>,
    ) -> Option<Hint> {
        let has_self = (self.cx.scopes.iter()).any(|s| s.contains_key(som_lang::SELF_VALUE));
        let found =
            foreign::value(name).or_else(|| foreign::self_word(name).filter(|_| has_self))?;
        let help = msg!["som writes ", code(found.som), ": ", found.hint()];
        Some((help, change_to(found.som.to_string())))
    }

    /// `range(n)` or `range(a, b)`, Python's count, or `puts(x)`, Ruby's print, where
    /// nothing's called that: reported, with som's way. Whether it was.
    pub(crate) fn foreign_call(&mut self, id: Id<Expr>, callee: &Path, args: &[Id<Expr>]) -> bool {
        let [segment] = callee.segments.as_slice() else {
            return false;
        };
        let name = &*segment.name;
        if self.lookup(name, Expect::Func).is_some() {
            return false;
        }
        let span = self.ast.exprs.span(id).clone();
        let text = |e: Id<Expr>| self.ast.exprs.span(e).get_text().to_string();
        let (help, fix) = if let Some(found) = foreign::len_function(name) {
            let [of] = args else {
                return false;
            };
            let read = format!("{}.{}", text(*of), som_lang::LEN);
            let help = msg!["som reads a length as ", code(&read), ": ", found.hint()];
            (help, Fix::change_to(span, read))
        } else if let Some(found) = foreign::range(name) {
            let range = match args {
                [end] => format!("0..{}", text(*end)),
                [start, end] => format!("{}..{}", text(*start), text(*end)),
                _ => return false,
            };
            let help = msg!["som writes a range ", code(&range), ": ", found.hint()];
            (help, Fix::change_to(span, range))
        } else if let Some(found) = foreign::print(name) {
            (
                println_hint(found),
                Fix::change_to(segment.span.clone(), PRINTLN),
            )
        } else {
            return false;
        };
        self.errors.push(ResolveError::UnknownFunction {
            name: name.to_string(),
            span: segment.span.clone(),
            help: Some(help),
            fixes: vec![fix],
        });
        true
    }

    /// `console.log(x)`, `System.out.println(x)`: printing as another language does, where
    /// nothing's called its first name. Reported, with som's `println`. Whether it was.
    pub(crate) fn foreign_print(&mut self, receiver: Id<Expr>, method: &Ident) -> bool {
        let mut names = vec![&*method.name];
        let mut at = receiver;
        let root = loop {
            match &self.ast.exprs[at] {
                Expr::Field { object, field } => {
                    names.push(&field.name);
                    at = *object;
                }
                Expr::Var(path) if path.segments.len() == 1 => break &path.segments[0],
                _ => return false,
            }
        };
        names.push(&root.name);
        names.reverse();
        let path = names.join(".");
        let Some(found) = foreign::print(&path) else {
            return false;
        };
        if self.lookup(&root.name, Expect::Value).is_some() {
            return false;
        }
        let span = self.ast.exprs.span(receiver).merge(&method.span);
        let fix = Fix::change_to(span.clone(), PRINTLN);
        self.errors.push(ResolveError::UnknownFunction {
            name: path,
            span,
            help: Some(println_hint(found)),
            fixes: vec![fix],
        });
        true
    }
}

fn println_hint(found: &foreign::Foreign) -> Message {
    msg![
        "som prints a line with ",
        code(format!("{PRINTLN}(…)")),
        ": ",
        found.hint()
    ]
}

/// A type named as another language names it: `int` for `i32`.
fn foreign_type_hint(name: &str, change_to: &dyn Fn(String) -> Vec<Fix>) -> Option<Hint> {
    let found = foreign::type_name(name)?;
    // `float`: `f32` or `f64`, depending on where it's from.
    if found.som.is_empty() {
        let help = msg![
            "som's floats are ",
            code("f32"),
            " and ",
            code("f64"),
            ": ",
            found.hint()
        ];
        return Some((help, Vec::new()));
    }
    let help = msg!["som writes ", code(found.som), ": ", found.hint()];
    Some((help, change_to(found.som.to_string())))
}

/// The best suggestion for a misspelled `name`: its snake_case spelling if that exists
/// (`InitWindow` for `init_window`, as imported C functions are named), otherwise the
/// closest name.
fn best_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let snake = snake_case(name);
    (candidates.iter())
        .find(|c| snake != name && **c == snake)
        .map(String::as_str)
        .or_else(|| closest_match(name, candidates, 2))
}
