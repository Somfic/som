//! C's `printf` formatting, for `snprintf` called while compiling: what the standard library's
//! `Display` uses to write numbers.

/// An argument after the format: a number, or the text a `%s` reads.
pub(crate) enum Arg {
    Int(i128),
    Float(f64),
    Text(Vec<u8>),
}

/// The bytes `format` makes of `args`, or `None` for a conversion it doesn't know, or an
/// argument of the wrong kind.
pub(crate) fn format(format: &[u8], args: &[Arg]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut i = 0;
    while i < format.len() {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        i += 1;
        // Flags, width, precision and length, then the conversion.
        let mut spec = Spec::default();
        while let Some(&c) = format.get(i) {
            match c {
                b'-' => spec.left = true,
                b'+' => spec.plus = true,
                b' ' => spec.space = true,
                b'0' => spec.zero = true,
                b'#' => spec.alternate = true,
                _ => break,
            }
            i += 1;
        }
        let number = |i: &mut usize| {
            let start = *i;
            while format.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            std::str::from_utf8(&format[start..*i])
                .ok()?
                .parse::<usize>()
                .ok()
        };
        spec.width = number(&mut i).unwrap_or(0);
        if format.get(i) == Some(&b'.') {
            i += 1;
            spec.precision = Some(number(&mut i).unwrap_or(0));
        }
        while matches!(format.get(i), Some(b'h' | b'l' | b'z' | b'j' | b't' | b'L')) {
            i += 1;
        }
        let conversion = *format.get(i)?;
        i += 1;
        let body = match conversion {
            b'%' => b"%".to_vec(),
            b'd' | b'i' => match args.next()? {
                Arg::Int(n) => spec.signed(*n < 0, n.unsigned_abs().to_string()),
                _ => return None,
            },
            b'u' | b'x' | b'X' | b'o' => match args.next()? {
                Arg::Int(n) => {
                    let n = *n as u128;
                    let digits = match conversion {
                        b'u' => n.to_string(),
                        b'x' => format!("{n:x}"),
                        b'X' => format!("{n:X}"),
                        _ => format!("{n:o}"),
                    };
                    spec.signed(false, digits)
                }
                _ => return None,
            },
            b'c' => match args.next()? {
                Arg::Int(n) => vec![*n as u8],
                _ => return None,
            },
            b's' => match args.next()? {
                Arg::Text(text) => match spec.precision {
                    Some(p) => text[..p.min(text.len())].to_vec(),
                    None => text.clone(),
                },
                _ => return None,
            },
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => match args.next()? {
                Arg::Float(x) => spec.float(*x, conversion),
                _ => return None,
            },
            _ => return None,
        };
        out.extend(spec.pad(body));
    }
    Some(out)
}

#[derive(Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    zero: bool,
    alternate: bool,
    width: usize,
    precision: Option<usize>,
}

impl Spec {
    /// A number's digits, with its sign, and zeroes up to the precision.
    fn signed(&self, negative: bool, mut digits: String) -> Vec<u8> {
        if let Some(p) = self.precision {
            if p == 0 && digits == "0" {
                digits.clear();
            }
            while digits.len() < p {
                digits.insert(0, '0');
            }
        }
        let sign = match (negative, self.plus, self.space) {
            (true, _, _) => "-",
            (false, true, _) => "+",
            (false, false, true) => " ",
            _ => "",
        };
        format!("{sign}{digits}").into_bytes()
    }

    fn float(&self, x: f64, conversion: u8) -> Vec<u8> {
        let upper = conversion.is_ascii_uppercase();
        let sign = match (x.is_sign_negative(), self.plus, self.space) {
            (true, _, _) => "-",
            (false, true, _) => "+",
            (false, false, true) => " ",
            _ => "",
        };
        let x = x.abs();
        let text = if x.is_nan() {
            "nan".to_string()
        } else if x.is_infinite() {
            "inf".to_string()
        } else {
            let precision = self.precision.unwrap_or(6);
            match conversion.to_ascii_lowercase() {
                b'f' => format!("{x:.precision$}"),
                b'e' => exponent(x, precision),
                _ => self.general(x, precision),
            }
        };
        let text = if upper { text.to_uppercase() } else { text };
        format!("{sign}{text}").into_bytes()
    }

    /// `%g`: `%e` if the exponent is below -4 or at least the precision, else `%f`, with the
    /// precision counting significant digits, and trailing zeroes dropped.
    fn general(&self, x: f64, precision: usize) -> String {
        let p = precision.max(1);
        // The exponent `%e` would show, after rounding to `p` digits.
        let e = if x == 0.0 {
            0
        } else {
            let shown = format!("{x:.*e}", p - 1);
            shown
                .split('e')
                .nth(1)
                .and_then(|e| e.parse::<i32>().ok())
                .unwrap_or(0)
        };
        let mut text = if e < -4 || e >= p as i32 {
            exponent(x, p - 1)
        } else {
            format!("{x:.*}", (p as i32 - 1 - e).max(0) as usize)
        };
        if !self.alternate {
            // Drop zeroes after the point, and the point if nothing's after it.
            let (number, exp) = match text.find('e') {
                Some(at) => (text[..at].to_string(), text[at..].to_string()),
                None => (text.clone(), String::new()),
            };
            let number = match number.contains('.') {
                true => number
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string(),
                false => number,
            };
            text = number + &exp;
        }
        text
    }

    /// `body`, padded to the width.
    fn pad(&self, body: Vec<u8>) -> Vec<u8> {
        if body.len() >= self.width {
            return body;
        }
        let fill = self.width - body.len();
        if self.left {
            return [body, vec![b' '; fill]].concat();
        }
        if self.zero {
            // Zeroes go after the sign.
            let sign = body.first().is_some_and(|c| b"+- ".contains(c)) as usize;
            return [&body[..sign], &vec![b'0'; fill], &body[sign..]].concat();
        }
        [vec![b' '; fill], body].concat()
    }
}

/// `%e`: `1.500000e+03`, with at least two exponent digits.
fn exponent(x: f64, precision: usize) -> String {
    let text = format!("{x:.precision$e}");
    let (number, e) = text.split_once('e').unwrap_or((&text, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    let sign = if e < 0 { '-' } else { '+' };
    format!("{number}e{sign}{:02}", e.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(fmt: &str, args: &[Arg]) -> String {
        String::from_utf8(format(fmt.as_bytes(), args).unwrap()).unwrap()
    }

    #[test]
    fn integers() {
        assert_eq!(f("%lld", &[Arg::Int(-42)]), "-42");
        assert_eq!(f("%llu", &[Arg::Int(7)]), "7");
        assert_eq!(
            f("[%5d|%-4d|%03d]", &[Arg::Int(1), Arg::Int(2), Arg::Int(3)]),
            "[    1|2   |003]"
        );
        assert_eq!(f("%x %X %%", &[Arg::Int(255), Arg::Int(255)]), "ff FF %");
    }

    #[test]
    fn floats_as_c_writes_them() {
        assert_eq!(f("%g", &[Arg::Float(3.5)]), "3.5");
        assert_eq!(f("%g", &[Arg::Float(100000.0)]), "100000");
        assert_eq!(f("%g", &[Arg::Float(1000000.0)]), "1e+06");
        assert_eq!(f("%g", &[Arg::Float(0.0001)]), "0.0001");
        assert_eq!(f("%g", &[Arg::Float(0.00001)]), "1e-05");
        assert_eq!(f("%g", &[Arg::Float(1.0 / 3.0)]), "0.333333");
        assert_eq!(f("%g", &[Arg::Float(0.0)]), "0");
        assert_eq!(f("%g", &[Arg::Float(-2.5)]), "-2.5");
        assert_eq!(f("%f", &[Arg::Float(1.5)]), "1.500000");
        assert_eq!(f("%e", &[Arg::Float(1500.0)]), "1.500000e+03");
    }

    #[test]
    fn text() {
        assert_eq!(f("%s!", &[Arg::Text(b"hi".to_vec())]), "hi!");
    }
}
