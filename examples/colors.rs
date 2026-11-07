//! How your colors read, over the glass: for choosing `opacity` and the palette
//! by numbers, not by eye alone. Run it in Kel, with your config:
//!
//!     cargo run --example colors
//!
//! Each color with its contrast against the background, whatever is behind the
//! window: a dark wallpaper, a gray one, a white page (the hard case). 4.5 and
//! more reads well, 3 is large text only, below that it's hard work. Then the
//! opacity each one needs over white, and rows as a diff shows them.

use kel_terminal::config;

const NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "br.black",
    "br.red",
    "br.green",
    "br.yellow",
    "br.blue",
    "br.magenta",
    "br.cyan",
    "br.white",
];

/// What can be behind the window.
const BEHIND: [u32; 3] = [0x101820, 0x808080, 0xffffff];

/// What reads well, by the usual rule (WCAG).
const GOOD: f64 = 4.5;

/// A color's lightness as the eye takes it, 0 to 1.
fn luminance(color: u32) -> f64 {
    let channel = |shift: u32| {
        let c = f64::from((color >> shift) & 0xff) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// 1 (the same) to 21 (black on white).
fn contrast(a: u32, b: u32) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `top`, `opacity` of it solid, over `behind`.
fn over(top: u32, behind: u32, opacity: f64) -> u32 {
    let channel = |shift: u32| {
        let (t, b) = ((top >> shift) & 0xff, (behind >> shift) & 0xff);
        (f64::from(t) * opacity + f64::from(b) * (1.0 - opacity)).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// The least opacity from which on `color` reads well on `background` over
/// white, if it does at all: counted down from solid, as a dark color reads
/// again on a clear enough window, but that's the white page it's read on.
fn opacity_for(color: u32, background: u32) -> Option<f64> {
    let reads = |n: u32| contrast(color, over(background, 0xffffff, f64::from(n) / 100.0)) >= GOOD;
    (0..=100)
        .rev()
        .take_while(|&n| reads(n))
        .last()
        .map(|n| f64::from(n) / 100.0)
}

/// `n` as a table's cell: with a `!` where it's hard to read.
fn cell(n: f64) -> String {
    format!("{n:5.1}{}", if n < 3.0 { "!" } else { " " })
}

fn rgb(color: u32) -> String {
    format!(
        "{};{};{}",
        color >> 16 & 0xff,
        color >> 8 & 0xff,
        color & 0xff
    )
}

fn main() {
    let config = config::load().unwrap_or_default();
    let theme = &config.colors;
    let background = theme.background;
    let opacity = match config.glass.on() {
        true => f64::from(config.glass.opacity),
        false => 1.0,
    };
    println!("Background #{background:06x}, opacity {opacity:.2}\n");
    println!(
        "{:<24}{:>7}{:>7}{:>7}{:>7}{:>10}",
        "", "solid", "dark", "gray", "white", "needs"
    );
    let line = |name: &str, code: &str, color: u32| {
        let on = |behind: u32| cell(contrast(color, over(background, behind, opacity)));
        let needs = match opacity_for(color, background) {
            Some(o) => format!("{o:.2}"),
            None => "never".to_string(),
        };
        println!(
            "\x1b[{code}m{name:<12}\x1b[0m #{color:06x}   {} {} {} {} {needs:>9}",
            cell(contrast(color, background)),
            on(BEHIND[0]),
            on(BEHIND[1]),
            on(BEHIND[2]),
        );
    };
    line("text", "39", theme.foreground);
    for (i, name) in NAMES.iter().enumerate() {
        let code = if i < 8 { 30 + i } else { 82 + i };
        line(name, &code.to_string(), theme.palette[i]);
    }
    println!("\n`needs`: the opacity at which it reads well over a white page.\n");

    println!("Backgrounds a program picks:");
    for row in 0..2 {
        for i in 0..8 {
            let code = if row == 0 { 40 + i } else { 100 + i };
            print!("\x1b[{code}m   {i}   \x1b[0m");
        }
        println!();
    }
    println!("\nRows as a diff shows them:");
    let rows: [(u32, &str, &str); 4] = [
        (0x3f1a20, "-", "the line that goes"),
        (0x5e2630, "-", "the words that changed"),
        (0x16391f, "+", "the line that comes"),
        (0x235a31, "+", "the words that changed"),
    ];
    for (color, sign, text) in rows {
        println!(
            "\x1b[48;2;{}m {sign} {text:<34} #{color:06x} \x1b[0m",
            rgb(color)
        );
    }
    println!("   a plain line between them");
}
