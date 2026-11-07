//! Glass: Kel see-through, Windows blurring what's behind it (the `[glass]` part
//! of the config). The theme's background and frame let some of the blur through,
//! the rest (text, lines, colors programs pick) stays solid. A fine grain on the
//! glass makes it read as frosted glass, not a hole in the window.
//!
//! How a color says it's see-through: its top byte, unused by the window's
//! pixels (`0x00RRGGBB`), is how *clear* it is: 0 solid (every color as it was),
//! 255 fully clear. The red, green and blue are already multiplied by how solid
//! it is ("premultiplied"), so blending two colors channel by channel, the top
//! byte too, gives the right color for an edge of a letter over glass. Before the
//! pixels go to Windows, `finish` turns the top byte into what Windows reads:
//! how solid (alpha).

use crate::theme::Theme;
use serde::Deserialize;
use std::sync::OnceLock;

/// What Windows puts behind the see-through parts.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    /// Nothing: Kel is solid, as without glass.
    #[default]
    None,
    /// What's behind the window, blurred (windows too). Windows 10 and 11.
    Acrylic,
    /// A faint tint of the wallpaper, like Windows 11's own apps. Windows 11.
    Mica,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub struct Glass {
    pub effect: Effect,
    /// How much of the theme's color covers the glass: 1 solid, 0 all glass.
    pub opacity: f32,
    /// How strong the grain is: 0 none, 1 strong.
    pub grain: f32,
    /// Under a full-screen program (lazygit) and Claude Code the terminal's
    /// background is solid, the frame still glass (see `App::solid_under`).
    pub solid_under_apps: bool,
    /// The programs that draw in the normal screen, not full-screen, and get
    /// it too (AI helpers, like Claude Code): see `wants_solid`.
    pub solid_for: Vec<String>,
    /// The color of that solid background. None (the default): the theme's.
    #[serde(deserialize_with = "crate::theme::optional_hex")]
    pub solid_color: Option<u32>,
    /// The backgrounds programs pick themselves (nvim's color scheme, lazygit's
    /// bars) are glass too, as much as the theme's. false: they stay solid.
    pub cells: bool,
}

impl Default for Glass {
    fn default() -> Self {
        Glass {
            // Acrylic: what's behind, blurred, with a light grain.
            effect: Effect::Acrylic,
            opacity: 0.85,
            grain: 0.1,
            solid_under_apps: true,
            solid_for: SOLID_FOR.iter().map(|name| name.to_string()).collect(),
            solid_color: None,
            cells: false,
        }
    }
}

/// `solid_for` by default: AI helpers in the terminal. Their answers are long,
/// in many colors, with frames: they read better solid.
const SOLID_FOR: [&str; 7] = [
    "claude", "codex", "gemini", "aider", "opencode", "copilot", "crush",
];

impl Glass {
    /// Whether the program running now is one of `solid_for`: by its name in
    /// the `command` that runs (`claude --resume`), or by a word of the
    /// `title` it gave the tab ("✳ Claude Code"). In any case.
    pub fn wants_solid(&self, command: Option<&str>, title: &str) -> bool {
        let listed = |word: &str| self.solid_for.iter().any(|n| n.eq_ignore_ascii_case(word));
        command.and_then(crate::icons::program).is_some_and(listed)
            || title.split(|c: char| !c.is_alphanumeric()).any(listed)
    }
}

/// How far a grain moves a pixel's brightness at `grain = 1`, out of 255.
const GRAIN_STRENGTH: f32 = 10.0;

/// The grain repeats every `TILE` pixels: too far apart to see it repeat.
const TILE: usize = 256;

/// Glass needs a system that can show it. Only Windows here: on macOS and Linux
/// the window's alpha doesn't reach the screen (softbuffer draws it solid), so
/// there the see-through colors would only darken the theme, and `finish` cost
/// 47 ms a frame at a Retina display's size, for nothing.
const SHOWS_GLASS: bool = cfg!(windows);

impl Glass {
    pub fn on(&self) -> bool {
        SHOWS_GLASS && self.effect != Effect::None
    }

    /// How solid the theme's background is over the glass, 0 to 255.
    fn alpha(&self) -> u32 {
        (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u32
    }

    /// A background a program picked, as the renderer paints it (see `cells`).
    pub fn picked_background(&self, color: u32) -> u32 {
        if self.on() && self.cells {
            see_through(color, self.alpha())
        } else {
            color
        }
    }

    /// `theme` as the renderer paints it: with glass, its background and frame
    /// see-through.
    pub fn theme(&self, theme: &Theme) -> Theme {
        let mut theme = theme.clone();
        if self.on() {
            theme.background = see_through(theme.background, self.alpha());
            theme.frame = see_through(theme.frame, self.alpha());
        }
        theme
    }

    /// Readies the pixels for Windows: the top byte from how clear to how solid,
    /// and the grain on the glass. Only with glass on: without it, the top byte
    /// stays 0 as Windows expects of a solid window.
    pub fn finish(&self, pixels: &mut [u32], width: usize) {
        // The glass's own clearness: a pixel this clear gets all the grain, a
        // letter's edge (less clear) a part of it, solid ones none.
        let glass = 255 - self.alpha();
        let strength = self.grain.clamp(0.0, 1.0) * GRAIN_STRENGTH;
        let grain = grain_tile();
        // This runs on every pixel of every frame, so whole numbers: what each
        // grain step moves a pixel on full glass, worked out once a frame.
        let move_by = |step: i32| step as f32 / 127.0 * strength;
        let steps: [i32; 255] = std::array::from_fn(|i| move_by(i as i32 - 127).round() as i32);
        for (y, row) in pixels.chunks_mut(width.max(1)).enumerate() {
            let tile_row = &grain[(y % TILE) * TILE..][..TILE];
            for piece in row.chunks_mut(TILE) {
                for (p, &step) in piece.iter_mut().zip(tile_row) {
                    let clear = *p >> 24;
                    if clear == 0 {
                        *p |= 0xff00_0000; // solid, most of a letter: no grain
                        continue;
                    }
                    let alpha = 255 - clear;
                    let mut rgb = *p & 0xff_ffff;
                    if glass > 0 && strength > 0.0 {
                        let d = if clear >= glass {
                            steps[(i32::from(step) + 127) as usize]
                        } else {
                            let part = clear as f32 / glass as f32;
                            (move_by(i32::from(step)) * part).round() as i32
                        };
                        // Premultiplied: a channel can't be more than alpha.
                        let ch = |s: u32| {
                            let c = ((rgb >> s) & 0xff) as i32 + d;
                            (c.clamp(0, alpha as i32) as u32) << s
                        };
                        rgb = ch(16) | ch(8) | ch(0);
                    }
                    *p = (alpha << 24) | rgb;
                }
            }
        }
    }
}

/// `color` see-through: `alpha` of it solid (0 to 255), premultiplied, its top
/// byte how clear.
pub fn see_through(color: u32, alpha: u32) -> u32 {
    let alpha = alpha.min(255);
    let ch = |s: u32| ((color >> s) & 0xff) * alpha / 255;
    ((255 - alpha) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Random brightness steps, -127 to 127, mostly small: the grain. The same
/// every time (a still grain, not a flicker), made once.
fn grain_tile() -> &'static [i8] {
    static TILE_CELLS: OnceLock<Vec<i8>> = OnceLock::new();
    TILE_CELLS.get_or_init(|| {
        (0..TILE * TILE)
            .map(|i| {
                let (x, y) = ((i % TILE) as i64, (i / TILE) as i64);
                // Two random numbers added: small steps often, big ones rarely.
                let a = crate::backdrop::unit(crate::backdrop::hash(1, x, y));
                let b = crate::backdrop::unit(crate::backdrop::hash(2, x, y));
                ((a + b - 1.0) * 127.0) as i8
            })
            .collect()
    })
}

/// Asks Windows for `effect` behind the window. The window must have been made
/// see-through when it was made (`with_transparent`): later, Windows shows black
/// instead of the glass, so a new effect takes a restart.
pub fn apply(window: &winit::window::Window, effect: Effect, background: u32) {
    #[cfg(target_os = "windows")]
    {
        // Mica has a light and a dark tint: the theme's.
        let (r, g, b) = (
            background >> 16 & 0xff,
            background >> 8 & 0xff,
            background & 0xff,
        );
        let dark = r * 299 + g * 587 + b * 114 < 128_000;
        // Errors: an older Windows without it. Then the see-through parts show
        // what's behind as it is, not blurred.
        let _ = match effect {
            Effect::None => Ok(()),
            Effect::Acrylic => window_vibrancy::apply_acrylic(window, None),
            Effect::Mica => window_vibrancy::apply_mica(window, Some(dark)),
        };
    }
    // Linux and macOS: no blur yet, the see-through parts show what's behind as it is.
    #[cfg(not(target_os = "windows"))]
    let _ = (window, effect, background);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_for_the_listed_programs() {
        let glass = Glass::default();
        assert!(glass.wants_solid(Some("claude --resume"), "powershell"));
        assert!(glass.wants_solid(None, "\u{2733} Claude Code"));
        assert!(glass.wants_solid(Some("/usr/local/bin/codex --resume"), ""));
        #[cfg(windows)]
        assert!(glass.wants_solid(Some("& 'C:\\tools\\Codex.exe'"), ""));
        assert!(!glass.wants_solid(Some("cargo build"), "powershell"));
        // Its program, not any word: a commit's message isn't a program.
        assert!(!glass.wants_solid(Some("git commit -m \"codex fix\""), "powershell"));
        // Your own, from the config.
        let mine = Glass {
            solid_for: vec!["btop".into()],
            ..Glass::default()
        };
        assert!(mine.wants_solid(Some("btop"), ""));
        assert!(!mine.wants_solid(Some("claude"), ""));
    }

    #[test]
    fn a_see_through_color() {
        // 0x1e (30) at 204/255 solid: 24, and 51 clear.
        assert_eq!(see_through(0x1e1e1e, 204), 0x3318_1818);
        assert_eq!(see_through(0x1e1e1e, 255), 0x001e_1e1e); // solid: as it was
        assert_eq!(see_through(0xffffff, 0), 0xff00_0000); // all glass
    }

    /// Where the window can't show it: no glass, whatever the config says.
    #[cfg(not(windows))]
    #[test]
    fn no_glass_where_it_can_t_show() {
        assert!(!Glass::default().on());
        let theme = Theme::default();
        assert_eq!(Glass::default().theme(&theme), theme);
    }

    #[cfg(windows)]
    #[test]
    fn glass_touches_only_the_background_and_frame() {
        let theme = Theme::default();
        let solid = Glass {
            effect: Effect::None,
            ..Glass::default()
        };
        assert_eq!(solid.theme(&theme), theme);
        let glass = Glass {
            effect: Effect::Acrylic,
            ..Glass::default()
        };
        let painted = glass.theme(&theme);
        assert_eq!(painted.background >> 24, 255 - 217); // 0.85 of 255: 217 solid
        assert_eq!(painted.frame >> 24, 255 - 217);
        assert_eq!(painted.foreground, theme.foreground);
        assert_eq!(painted.palette, theme.palette);
    }

    #[cfg(windows)]
    #[test]
    fn a_program_s_background() {
        let mut glass = Glass {
            opacity: 0.8,
            ..Glass::default()
        };
        assert_eq!(glass.picked_background(0x1e1e1e), 0x1e1e1e); // solid by default
        glass.cells = true;
        assert_eq!(
            glass.picked_background(0x1e1e1e),
            see_through(0x1e1e1e, 204)
        );
        glass.effect = Effect::None;
        assert_eq!(glass.picked_background(0x1e1e1e), 0x1e1e1e); // no glass at all
    }

    #[test]
    fn finishing_turns_clear_into_alpha() {
        let glass = Glass {
            effect: Effect::Acrylic,
            opacity: 0.8,
            grain: 0.0,
            ..Glass::default()
        };
        let mut pixels = [0x00d4_d4d4, see_through(0x1e1e1e, 204)];
        glass.finish(&mut pixels, 2);
        assert_eq!(pixels, [0xffd4_d4d4, 0xcc18_1818]);
    }

    #[test]
    fn the_grain_is_on_the_glass_only() {
        let glass = Glass {
            effect: Effect::Acrylic,
            opacity: 0.8,
            grain: 1.0,
            ..Glass::default()
        };
        let back = see_through(0x1e1e1e, 204);
        let mut pixels = vec![back; 64];
        pixels[0] = 0x00d4_d4d4; // a letter: solid
        glass.finish(&mut pixels, 8);
        assert_eq!(pixels[0], 0xffd4_d4d4);
        // The glass: the same alpha, brightness a few steps either way, not all alike.
        let glassy = &pixels[1..];
        assert!(glassy.iter().all(|p| p >> 24 == 204));
        assert!(glassy.iter().all(|p| (p & 0xff).abs_diff(0x18) <= 10));
        assert!(glassy.iter().any(|&p| p != glassy[0]));
    }
}
