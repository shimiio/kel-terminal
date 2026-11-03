//! The start screen's background: ASCII drawn behind the logo and the list, dim,
//! fading out around them. By day a terrain, like a map of heights; at night,
//! stars. New each time home shows its start screen: a seed picks it.
//! `backdrop` in the config picks another picture: contours, mountains, the
//! sea, wind; or none.
//!
//! ```text
//!  ==++++===---- .     .:--==++**+==-      +
//!  ===+==-- .              .     *     .
//! ```
//!
//! Nothing is kept but the seed: the mark at a cell comes from the seed and the
//! cell's place, the same every time it's asked. So any window size is drawn,
//! and the picture holds still while the screen is up.

use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Which picture: `backdrop` in the config.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Choice {
    /// A terrain by day, stars at night.
    #[default]
    Auto,
    Terrain,
    Contours,
    Mountains,
    Sea,
    Wind,
    Stars,
    /// Nothing: the start screen plain.
    None,
}

impl Choice {
    /// As the config spells it.
    pub fn name(self) -> &'static str {
        match self {
            Choice::Auto => "auto",
            Choice::Terrain => "terrain",
            Choice::Contours => "contours",
            Choice::Mountains => "mountains",
            Choice::Sea => "sea",
            Choice::Wind => "wind",
            Choice::Stars => "stars",
            Choice::None => "none",
        }
    }
}

/// What's drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Smooth noise as characters, from ` ` (low) to `*` (high).
    Terrain,
    /// The same heights as lines of one height, like a hiking map: `-` `/` `|` `\`.
    Contours,
    /// Three ranges of mountains at the bottom, the far ones dimmer.
    Mountains,
    /// A horizon, and waves below it, bigger and brighter nearer.
    Sea,
    /// Short strokes turning with the wind, like grass or iron filings.
    Wind,
    /// A few `.` `+` `*` here and there.
    Stars,
}

impl Kind {
    /// What the hour of the day draws: stars at night (22:00 to 6:00), terrain
    /// by day.
    pub fn at(hour: u32) -> Kind {
        if is_night(hour) {
            Kind::Stars
        } else {
            Kind::Terrain
        }
    }
}

/// A cell's mark: its character, and how bright (out of 255: how much of the
/// text's color over the background).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub ch: char,
    pub strength: u8,
}

/// The terrain's characters, from low to high.
const RAMP: &[u8] = b"..::--==++**";

/// The terrain's heights that show, from `SHORE` (the lowest, faint) to
/// `PEAK` (the highest, `*`). Below the shore: nothing, the air between hills.
/// The heights crowd around the middle (two noises added), so these are
/// closer than 0 and 1.
const SHORE: f64 = 0.47;
const PEAK: f64 = 0.8;

pub struct Backdrop {
    kind: Kind,
    /// Night: stars in the sky of the mountains and the sea.
    night: bool,
    seed: u64,
}

impl Backdrop {
    pub fn new(kind: Kind, night: bool, seed: u64) -> Backdrop {
        Backdrop { kind, night, seed }
    }

    /// One for now: `choice`'s picture (auto: by the hour of the day here),
    /// from a seed never the same. `None`: no picture.
    pub fn now(choice: Choice) -> Option<Backdrop> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        let hour = local_hour();
        let kind = match choice {
            Choice::Auto => Kind::at(hour),
            Choice::Terrain => Kind::Terrain,
            Choice::Contours => Kind::Contours,
            Choice::Mountains => Kind::Mountains,
            Choice::Sea => Kind::Sea,
            Choice::Wind => Kind::Wind,
            Choice::Stars => Kind::Stars,
            Choice::None => return None,
        };
        Some(Backdrop::new(kind, is_night(hour), nanos))
    }

    /// The mark at column `col`, row `row`, if there is one; `rows`: how many
    /// there are (the mountains stand on the bottom, the sea's horizon is in
    /// the middle).
    pub fn mark(&self, col: usize, row: usize, rows: usize) -> Option<Mark> {
        let (x, y) = (col as f64, row as f64);
        match self.kind {
            Kind::Terrain => {
                let height = (self.height(x, y) - SHORE) / (PEAK - SHORE);
                if height < 0.0 {
                    return None;
                }
                let i = ((height * RAMP.len() as f64) as usize).min(RAMP.len() - 1);
                // Higher is brighter: 5% to 18% of the text's color.
                let strength = (12.0 + 34.0 * height.min(1.0)) as u8;
                Some(Mark {
                    ch: RAMP[i] as char,
                    strength,
                })
            }
            Kind::Contours => self.contour(x, y),
            Kind::Mountains => self.mountains(col, row, rows),
            Kind::Sea => self.sea(col, row, rows),
            Kind::Wind => self.wind(col, row),
            Kind::Stars => self.star(col, row),
        }
    }

    /// The terrain's height at (x, y), 0 to 1. Cells are about twice as tall
    /// as wide: rows change faster. A finer layer over the big one, for detail.
    fn height(&self, x: f64, y: f64) -> f64 {
        let big = noise(self.seed, x / 12.0, y / 6.0);
        let fine = noise(self.seed ^ 0x9e37, x / 5.0, y / 2.5);
        0.78 * big + 0.22 * fine
    }

    fn star(&self, col: usize, row: usize) -> Option<Mark> {
        let p = unit(hash(self.seed ^ 0x57a2, col as i64, row as i64));
        let (ch, strength) = match p {
            p if p < 0.004 => ('*', 56),
            p if p < 0.015 => ('+', 38),
            p if p < 0.05 => ('.', 33),
            _ => return None,
        };
        Some(Mark { ch, strength })
    }

    /// The sky: stars at night, empty by day.
    fn sky(&self, col: usize, row: usize) -> Option<Mark> {
        self.night.then(|| self.star(col, row)).flatten()
    }

    /// A line where the height crosses one of `LEVELS` steps, as the stroke
    /// going along it. Every third step brighter, as on a map.
    fn contour(&self, x: f64, y: f64) -> Option<Mark> {
        const LEVELS: f64 = 5.0;
        // Smoother than the terrain: its fine layer would tangle the lines.
        let height = |x: f64, y: f64| noise(self.seed, x / 22.0, y / 11.0);
        let level = |x: f64, y: f64| (height(x, y) * LEVELS).floor();
        let here = level(x, y);
        let (right, below) = (level(x + 1.0, y), level(x, y + 1.0));
        if here == right && here == below {
            return None;
        }
        // Which way the height climbs, in what the eye sees: a row is about
        // two columns tall. The line goes across it.
        let gx = height(x + 1.0, y) - height(x - 1.0, y);
        let gy = (height(x, y + 1.0) - height(x, y - 1.0)) / 2.0;
        let ch = match () {
            _ if gx.abs() > 2.0 * gy.abs() => '|',
            _ if gy.abs() > 2.0 * gx.abs() => '-',
            // Climbing right and down (y grows down): the line goes up-right.
            _ if gx * gy > 0.0 => '/',
            _ => '\\',
        };
        let step = here.max(right).max(below) as i64;
        let strength = if step % 3 == 0 { 40 } else { 24 };
        Some(Mark { ch, strength })
    }

    /// Three ranges, nearest first: a cell is the nearest one's that covers it.
    /// The ridge is a line of `/` `\` `^` `-`, below it a few dots; above the
    /// farthest, the sky.
    fn mountains(&self, col: usize, row: usize, rows: usize) -> Option<Mark> {
        // (its foot, of the height; how tall, of the height; how wide a hill,
        // in columns; how bright)
        const RANGES: [(f64, f64, f64, u8); 3] = [
            (1.0, 0.30, 7.0, 42),
            (0.85, 0.34, 11.0, 30),
            (0.68, 0.34, 16.0, 20),
        ];
        let x = col as f64;
        for (k, &(foot, tall, wide, strength)) in RANGES.iter().enumerate() {
            let seed = self.seed ^ (k as u64 + 1).wrapping_mul(0x51ed_270b);
            let top = |x: f64| (rows as f64 * (foot - tall * ridge(seed, x / wide))).round() as i64;
            let (left, here, right) = (top(x - 1.0), top(x), top(x + 1.0));
            let row = row as i64;
            if row < here {
                continue; // above it: a farther one, or the sky
            }
            // The line goes down to a lower neighbor, so a steep side has no gaps.
            let bottom = here.max(left.max(right) - 1);
            if row <= bottom {
                let ch = match () {
                    _ if row == here && left > here && right > here => '^',
                    _ if row > here && right > left => '\\',
                    _ if row > here => '/',
                    _ if right > left => '\\',
                    _ if right < left => '/',
                    _ => '-',
                };
                return Some(Mark { ch, strength });
            }
            // Inside: a dot here and there, more on the near ones.
            let dense = 0.02 + 0.03 * (2 - k) as f64;
            let p = unit(hash(seed, col as i64, row));
            return (p < dense).then_some(Mark {
                ch: '.',
                strength: strength * 2 / 3,
            });
        }
        self.sky(col, row)
    }

    /// The horizon a bit above the middle; under it, rows of waves, sparse
    /// and small far away, bigger and brighter nearer.
    fn sea(&self, col: usize, row: usize, rows: usize) -> Option<Mark> {
        let horizon = rows * 2 / 5;
        if row < horizon {
            return self.sky(col, row);
        }
        let (x, y) = (col as f64, row as f64);
        if row == horizon {
            let n = noise(self.seed ^ 0x40, x / 3.0, 0.0);
            return (n > 0.3).then_some(Mark {
                ch: '_',
                strength: 26,
            });
        }
        // How near, 0 at the horizon to 1 at the bottom.
        let near = (row - horizon) as f64 / (rows - horizon).max(1) as f64;
        // Each row its own waves, longer nearer.
        let n = noise(self.seed, x / (1.2 + 3.0 * near), y * 3.7);
        if n < 0.7 - 0.12 * near {
            return None;
        }
        let ch = if near < 0.2 { '-' } else { '~' };
        let strength = (16.0 + 30.0 * near) as u8;
        Some(Mark { ch, strength })
    }

    /// Half the cells: a stroke along the wind there, which turns slowly
    /// across the screen.
    fn wind(&self, col: usize, row: usize) -> Option<Mark> {
        if unit(hash(self.seed ^ 0x77, col as i64, row as i64)) > 0.2 {
            return None;
        }
        let (x, y) = (col as f64, row as f64);
        // Its direction, 0 to half a turn (a stroke has two ends alike):
        // mostly along, tilting up and down.
        let turn = ((noise(self.seed, x / 30.0, y / 15.0) - 0.5) * 0.8).rem_euclid(1.0);
        let ch = match turn {
            t if !(0.0625..0.9375).contains(&t) => '-',
            t if t < 0.3125 => '/',
            t if t < 0.6875 => '|',
            _ => '\\',
        };
        let strength = (14.0 + 26.0 * noise(self.seed ^ 0x99, x / 6.0, y / 3.0)) as u8;
        Some(Mark { ch, strength })
    }
}

/// A row of hills from 0 to 1 along `x`: broad ones, and sharp peaks on them
/// (a fold of a finer noise: sharp where it turns).
fn ridge(seed: u64, x: f64) -> f64 {
    let broad = noise(seed, x, 0.5);
    let sharp = 1.0 - (2.0 * noise(seed ^ 0xabc, x * 2.2, 0.5) - 1.0).abs();
    0.55 * broad + 0.45 * sharp
}

/// Smooth noise at (x, y), 0 to 1: random heights at whole x and y, and
/// between them a smooth way from one to the next.
fn noise(seed: u64, x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let corner = |dx: i64, dy: i64| unit(hash(seed, ix + dx, iy + dy));
    // Slow at the ends, fast in the middle: no corners where squares meet.
    let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(x - ix as f64), smooth(y - iy as f64));
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * sx;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * sx;
    top + (bottom - top) * sy
}

/// A number that looks random, the same for the same seed and place
/// (SplitMix64's mixing).
pub(crate) fn hash(seed: u64, x: i64, y: i64) -> u64 {
    let mut z = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `hash` as a number from 0 to 1.
pub(crate) fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Night: 22:00 to 6:00.
fn is_night(hour: u32) -> bool {
    !(6..22).contains(&hour)
}

/// The hour on the clock here, 0 to 23.
#[cfg(windows)]
fn local_hour() -> u32 {
    /// Windows' SYSTEMTIME: the date and time, each a 16-bit number.
    #[repr(C)]
    #[derive(Default)]
    struct WinTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(time: *mut WinTime);
    }
    let mut time = WinTime::default();
    // Safety: `WinTime` is laid out as SYSTEMTIME, which Windows fills.
    unsafe { GetLocalTime(&mut time) };
    u32::from(time.hour)
}

/// Linux and macOS: UTC for now (a time zone needs more than std has).
#[cfg(not(windows))]
fn local_hour() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (secs / 3600 % 24) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_is_stars() {
        assert_eq!(Kind::at(21), Kind::Terrain);
        assert_eq!(Kind::at(22), Kind::Stars);
        assert_eq!(Kind::at(0), Kind::Stars);
        assert_eq!(Kind::at(5), Kind::Stars);
        assert_eq!(Kind::at(6), Kind::Terrain);
        assert!(local_hour() < 24);
    }

    #[test]
    fn a_seed_is_one_picture() {
        let marks =
            |b: &Backdrop| -> Vec<_> { (0..2000).map(|i| b.mark(i % 80, i / 80, 25)).collect() };
        let a = Backdrop::new(Kind::Terrain, false, 7);
        assert_eq!(marks(&a), marks(&Backdrop::new(Kind::Terrain, false, 7)));
        assert_ne!(marks(&a), marks(&Backdrop::new(Kind::Terrain, false, 8)));
        // The terrain: only its characters, 5% to 18% bright; and air between
        // its hills, a good part of the screen empty.
        for mark in marks(&a).into_iter().flatten() {
            assert!(RAMP.contains(&(mark.ch as u8)));
            assert!((12..=46).contains(&mark.strength));
        }
        let empty = marks(&a).iter().filter(|m| m.is_none()).count();
        assert!((500..1500).contains(&empty), "{empty}");
    }

    #[test]
    fn every_picture_draws_something() {
        for kind in [Kind::Contours, Kind::Mountains, Kind::Sea, Kind::Wind] {
            let b = Backdrop::new(kind, true, 3);
            let lit = (0..4000)
                .filter(|i| b.mark(i % 100, i / 100, 40).is_some())
                .count();
            assert!((100..3000).contains(&lit), "{kind:?}: {lit}");
        }
        // The config's "none": nothing at all.
        assert!(Backdrop::now(Choice::None).is_none());
        assert!(Backdrop::now(Choice::Sea).is_some());
    }

    #[test]
    fn stars_are_few() {
        let stars = Backdrop::new(Kind::Stars, true, 42);
        let lit = (0..10_000)
            .filter(|i| stars.mark(i % 100, i / 100, 100).is_some())
            .count();
        // About 5% of the cells.
        assert!((200..900).contains(&lit), "{lit}");
    }
}
