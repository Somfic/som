use owo_colors::OwoColorize;
pub use owo_colors::Rgb;

// Catppuccin Mocha.
pub const MAUVE: Rgb = Rgb(203, 166, 247);
pub const RED: Rgb = Rgb(243, 139, 168);
pub const PEACH: Rgb = Rgb(250, 179, 135);
pub const YELLOW: Rgb = Rgb(249, 226, 175);
pub const GREEN: Rgb = Rgb(166, 227, 161);
pub const TEAL: Rgb = Rgb(148, 226, 213);
pub const SKY: Rgb = Rgb(137, 220, 235);
pub const BLUE: Rgb = Rgb(137, 180, 250);
pub const LAVENDER: Rgb = Rgb(180, 190, 254);
pub const TEXT: Rgb = Rgb(205, 214, 244);
pub const SUBTEXT0: Rgb = Rgb(166, 173, 200);
pub const SURFACE2: Rgb = Rgb(88, 91, 112);

fn fg(text: impl std::fmt::Display, color: Rgb) -> String {
    text.to_string()
        .truecolor(color.0, color.1, color.2)
        .to_string()
}

pub(super) fn teal(text: impl std::fmt::Display) -> String {
    fg(text, TEAL)
}

// Dim like `subtext`, but upright.
pub(super) fn margin(text: impl std::fmt::Display) -> String {
    fg(text, SUBTEXT0)
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
    fg(text, SURFACE2)
}

pub(super) const ITALIC: &str = "\x1b[3m";

// Each reset inside `text`, after a coloured name, turns `style` back on.
pub(super) fn styled(text: &str, style: &str, color: bool) -> String {
    match color {
        true => format!(
            "{style}{}\x1b[0m",
            text.replace("\x1b[0m", &format!("\x1b[0m{style}"))
        ),
        false => text.to_string(),
    }
}
