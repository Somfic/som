use owo_colors::OwoColorize;
pub use owo_colors::Rgb;

// Catppuccin Mocha.
pub const ROSEWATER: Rgb = Rgb(245, 224, 220);
pub const FLAMINGO: Rgb = Rgb(242, 205, 205);
pub const PINK: Rgb = Rgb(245, 194, 231);
pub const MAUVE: Rgb = Rgb(203, 166, 247);
pub const RED: Rgb = Rgb(243, 139, 168);
pub const MAROON: Rgb = Rgb(235, 160, 172);
pub const PEACH: Rgb = Rgb(250, 179, 135);
pub const YELLOW: Rgb = Rgb(249, 226, 175);
pub const GREEN: Rgb = Rgb(166, 227, 161);
pub const TEAL: Rgb = Rgb(148, 226, 213);
pub const SKY: Rgb = Rgb(137, 220, 235);
pub const SAPPHIRE: Rgb = Rgb(116, 199, 236);
pub const BLUE: Rgb = Rgb(137, 180, 250);
pub const LAVENDER: Rgb = Rgb(180, 190, 254);
pub const TEXT: Rgb = Rgb(205, 214, 244);
pub const SUBTEXT1: Rgb = Rgb(186, 194, 222);
pub const SUBTEXT0: Rgb = Rgb(166, 173, 200);
pub const OVERLAY2: Rgb = Rgb(147, 153, 178);
pub const OVERLAY1: Rgb = Rgb(127, 132, 156);
pub const OVERLAY0: Rgb = Rgb(108, 112, 134);
pub const SURFACE2: Rgb = Rgb(88, 91, 112);
pub const SURFACE1: Rgb = Rgb(69, 71, 90);
pub const SURFACE0: Rgb = Rgb(49, 50, 68);
pub const BASE: Rgb = Rgb(30, 30, 46);
pub const MANTLE: Rgb = Rgb(24, 24, 37);
pub const CRUST: Rgb = Rgb(17, 17, 27);

pub(super) fn teal(text: impl std::fmt::Display) -> String {
    format!("{}", text.to_string().truecolor(TEAL.0, TEAL.1, TEAL.2))
}

/// A line number in the margin: dim, but upright, unlike `subtext`.
pub(super) fn margin(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SUBTEXT0.0, SUBTEXT0.1, SUBTEXT0.2)
    )
}

pub(super) fn subtext(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SUBTEXT0.0, SUBTEXT0.1, SUBTEXT0.2)
            .italic()
    )
}

pub(super) fn surface(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SURFACE2.0, SURFACE2.1, SURFACE2.2)
    )
}

pub(super) const ITALIC: &str = "\x1b[3m";

/// `text` in `style` (an escape code, like `ITALIC`) throughout, when in `color`: each reset
/// inside it, after a name in colour, turns the style back on.
pub(super) fn styled(text: &str, style: &str, color: bool) -> String {
    match color {
        true => format!(
            "{style}{}\x1b[0m",
            text.replace("\x1b[0m", &format!("\x1b[0m{style}"))
        ),
        false => text.to_string(),
    }
}
