/// How many statements code run while compiling may take, for one constant.
pub const STEP_LIMIT: u64 = 10_000_000;

/// The deepest calls can go while compiling.
pub(crate) const DEPTH_LIMIT: usize = 10_000;

/// The most steps a call that could be `@` one is tried for: more, and it's no call to make
/// while compiling (and the editor tries them on every check).
pub(crate) const TRY_LIMIT: u64 = 1_000_000;

/// The fewest steps a call takes for it to be worth working out while compiling:
/// `factorial(5)` is gone in a moment when the program runs.
pub(crate) const MIN_STEPS: u64 = 1_000;
