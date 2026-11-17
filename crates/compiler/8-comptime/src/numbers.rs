//! Arithmetic as the program does it when it runs.

use som_mir::{BinOp, FloatTy, IntTy};

pub(crate) fn round(x: f64, float: FloatTy) -> f64 {
    match float {
        FloatTy::F32 => (x as f32) as f64,
        FloatTy::F64 => x,
    }
}

/// `usize` and `isize` are the host's width, like the C backend's.
fn bits(int: IntTy) -> u32 {
    match int {
        IntTy::ISize | IntTy::USize => usize::BITS,
        _ => int.bits(),
    }
}

/// The smallest and largest `int`. A `u128` past `i128::MAX` can't be held.
pub(crate) fn bounds(int: IntTy) -> (i128, i128) {
    match (bits(int), int.is_signed()) {
        (128, _) => (i128::MIN, i128::MAX),
        (bits, true) => (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1),
        (bits, false) => (0, (1i128 << bits) - 1),
    }
}

/// `n` wrapped to `int`'s width.
pub(crate) fn wrap(n: i128, int: IntTy) -> i128 {
    let bits = bits(int);
    if bits == 128 {
        return n;
    }
    let modulus = 1i128 << bits;
    let min = if int.is_signed() { -modulus / 2 } else { 0 };
    n.wrapping_sub(min).rem_euclid(modulus) + min
}

/// Whether `a / b` is `MIN / -1` in a signed type, which traps.
pub(crate) fn traps(a: i128, b: i128, int: IntTy) -> bool {
    int.is_signed() && b == -1 && a == bounds(int).0
}

/// `a << n` or `a >> n`, as the optimizer folds them: moved its width or more, or a negative
/// amount, none of `a`'s bits are left.
pub(crate) fn shift(op: BinOp, a: i128, n: i128, int: IntTy) -> i128 {
    let bits = bits(int) as i128;
    if !(0..bits).contains(&n) {
        return if op == BinOp::ShiftRight && a < 0 {
            -1
        } else {
            0
        };
    }
    let moved = match op {
        BinOp::ShiftLeft => ((a as u128) << n) as i128,
        _ => a >> n,
    };
    wrap(moved, int)
}
