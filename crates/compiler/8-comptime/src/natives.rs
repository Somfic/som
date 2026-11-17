//! The C functions the standard library's text uses, done here rather than called: calling
//! them changes nothing outside the program.

use crate::machine::Machine;
use crate::memory::Byte;
use crate::{Loc, Stop, Value, count, out_of_bounds, pointer, printf, unsupported};
use som_base::Span;

type Native = fn(&mut Machine<'_, '_>, &[Value], &Span) -> Result<Value, Stop>;

const NATIVES: &[(&str, Native)] = &[
    ("memcpy", copy),
    ("memmove", copy),
    ("memset", set),
    ("memcmp", compare),
    ("strlen", strlen),
    ("snprintf", snprintf),
];

pub(crate) fn is_native(name: &str) -> bool {
    NATIVES.iter().any(|&(native, _)| native == name)
}

impl Machine<'_, '_> {
    /// C function `name`, done here: its result, or `None` if it isn't one of these.
    pub(crate) fn native(
        &mut self,
        name: &str,
        args: &[Value],
        span: &Span,
    ) -> Result<Option<Value>, Stop> {
        match NATIVES.iter().find(|&&(native, _)| native == name) {
            Some((_, run)) => run(self, args, span).map(Some),
            None => Ok(None),
        }
    }
}

/// Where pointer `value` points, which mustn't be null.
fn target(value: &Value, span: &Span) -> Result<Loc, Stop> {
    match pointer(value, span)? {
        Some(loc) => Ok(loc),
        None => out_of_bounds(span),
    }
}

fn copy(m: &mut Machine, args: &[Value], span: &Span) -> Result<Value, Stop> {
    let (dst, src, n) = (
        target(&args[0], span)?,
        target(&args[1], span)?,
        count(&args[2], span)?,
    );
    // All read first: they can overlap.
    let bytes = (0..n)
        .map(|i| m.byte(&src, i, span))
        .collect::<Result<Vec<_>, _>>()?;
    for (i, byte) in bytes.into_iter().enumerate() {
        m.set_byte(&dst, i, byte, span)?;
    }
    Ok(args[0].clone())
}

fn set(m: &mut Machine, args: &[Value], span: &Span) -> Result<Value, Stop> {
    let (dst, n) = (target(&args[0], span)?, count(&args[2], span)?);
    let Value::Int(c) = args[1] else {
        return unsupported("this byte", span);
    };
    for i in 0..n {
        m.set_byte(&dst, i, Byte::Data(c as u8), span)?;
    }
    Ok(args[0].clone())
}

fn compare(m: &mut Machine, args: &[Value], span: &Span) -> Result<Value, Stop> {
    let n = count(&args[2], span)?;
    let a = m.data(&target(&args[0], span)?, n, span)?;
    let b = m.data(&target(&args[1], span)?, n, span)?;
    Ok(Value::Int(a.cmp(&b) as i128))
}

fn strlen(m: &mut Machine, args: &[Value], span: &Span) -> Result<Value, Stop> {
    Ok(Value::Int(
        m.c_text(&target(&args[0], span)?, span)?.len() as i128
    ))
}

fn snprintf(m: &mut Machine, args: &[Value], span: &Span) -> Result<Value, Stop> {
    let (size, format) = (
        count(&args[1], span)?,
        m.c_text(&target(&args[2], span)?, span)?,
    );
    let mut rest = Vec::new();
    for arg in &args[3..] {
        rest.push(match arg {
            Value::Int(n) => printf::Arg::Int(*n),
            Value::Bool(b) => printf::Arg::Int(*b as i128),
            Value::Float(x) => printf::Arg::Float(*x),
            Value::Ref(loc) => printf::Arg::Text(m.c_text(loc, span)?),
            _ => return unsupported("this argument to `snprintf`", span),
        });
    }
    let Some(text) = printf::format(&format, &rest) else {
        return unsupported("this `snprintf` format", span);
    };
    // As much as fits, then the zero byte.
    if size > 0 {
        let dst = target(&args[0], span)?;
        let shown = text.len().min(size - 1);
        for (i, &b) in text[..shown].iter().enumerate() {
            m.set_byte(&dst, i, Byte::Data(b), span)?;
        }
        m.set_byte(&dst, shown, Byte::Data(0), span)?;
    }
    Ok(Value::Int(text.len() as i128))
}
