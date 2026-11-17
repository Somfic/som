named_enum! {
    /// The compiler's own `@` words. Any other `@name(…)` is a function called while
    /// compiling.
    pub enum Builtin {
        Unreachable = "@unreachable",
        Fail = "@fail",
        On = "@on",
        Test = "@test",
        For = "@for",
        If = "@if",
        Const = "@const",
        Where = "@where",
    }
}

impl Builtin {
    /// What an editor shows for it. `None` for what isn't suggested on its own.
    pub fn doc(self) -> Option<&'static str> {
        Some(match self {
            Builtin::Unreachable => {
                "A place the program can't get to, standing for a value of whatever type is \
                 expected there: `get(i) else @unreachable`. If it's reached after all, it says \
                 where, then stops the program."
            }
            Builtin::Fail => {
                "Stops with a message: `@fail(\"no room\")`. When the program runs, it says what \
                 failed and where, then stops it; in a `@const`, it's an error while compiling. \
                 Like `@unreachable`, it stands for a value of whatever type is expected there."
            }
            Builtin::On => {
                "On a line above a function: an event it's called on. `@on fail`: when the \
                 program fails, at `@fail` or `@unreachable`, with a `&Failure` saying what and \
                 where. The program's own listeners are called instead of the standard \
                 library's printer."
            }
            Builtin::Test => {
                "On a line above a function: a test, run by `som test` and left out of the \
                 program otherwise. It takes nothing and gives nothing back; it passes unless it \
                 fails, at `assert`, `@fail` or `@unreachable`."
            }
            Builtin::For => {
                "A loop settled while compiling: `@for i in 0..4 { s = s + v[i]; }` is the body \
                 four times over, with `i` 0, 1, 2 and 3. What it loops over has to be known \
                 while compiling: a range of known numbers, or an array literal."
            }
            Builtin::If => {
                "A choice made while compiling: `@if DEBUG { log(message); }`. Only the branch \
                 the condition picks is checked and compiled; the condition has to be known \
                 while compiling."
            }
            Builtin::Const => {
                "Worked out while compiling: a constant, `@const CELLS = COLS * ROWS;`, or a \
                 value where it's written, `@const first_primes()`."
            }
            Builtin::Where => return None,
        })
    }
}
