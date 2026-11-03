//! The start screen's background: ASCII drawn behind the logo and the list, dim,
//! fading out around them. By day a terrain, like a map of heights; at night,
//! stars. New each time home shows its start screen: a seed picks it.
//!
//! ```text
//!  ==++++===---- .     .:--==++**+==-      +
//!  ===+==-- .              .     *     .
//! ```
//!
//! Nothing is kept but the seed: the mark at a cell comes from the seed and the
//! cell's place, the same every time it's asked. So any window size is drawn,
//! and the picture holds still while the screen is up.

use std::time::{SystemTime, UNIX_EPOCH};

/// What's drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// Smooth noise as characters, from ` ` (low) to `*` (high).
    Terrain,
    /// A few `.` `+` `*` here and there.
    Stars,
}

impl Kind {
    /// What the hour of the day draws: stars at night (22:00 to 6:00), terrain
    /// by day.
    pub fn at(hour: u32) -> Kind {
        if !(6..22).contains(&hour) {
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
const RAMP: &[u8] = b" ..::--==++**";

pub struct Backdrop {
    kind: Kind,
    seed: u64,
}

impl Backdrop {
    pub fn new(kind: Kind, seed: u64) -> Backdrop {
        Backdrop { kind, seed }
    }

    /// One for now: by the hour of the day here, from a seed never the same.
    pub fn now() -> Backdrop {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Backdrop::new(Kind::at(local_hour()), nanos)
    }

    /// The mark at column `col`, row `row`, if there is one.
    pub fn mark(&self, col: usize, row: usize) -> Option<Mark> {
        let (x, y) = (col as f64, row as f64);
        match self.kind {
            Kind::Terrain => {
                // Cells are about twice as tall as wide: rows change faster.
                // A finer layer over the big one, for detail.
                let big = noise(self.seed, x / 8.0, y / 4.0);
                let fine = noise(self.seed ^ 0x9e37, x / 4.0, y / 2.0);
                let height = 0.75 * big + 0.25 * fine;
                let i = ((height * RAMP.len() as f64) as usize).min(RAMP.len() - 1);
                let ch = RAMP[i] as char;
                // Higher is brighter: 6% to 16% of the text's color.
                let strength = (15.0 + 26.0 * height) as u8;
                (ch != ' ').then_some(Mark { ch, strength })
            }
            Kind::Stars => {
                let p = unit(hash(self.seed, col as i64, row as i64));
                let (ch, strength) = match p {
                    p if p < 0.004 => ('*', 56),
                    p if p < 0.015 => ('+', 38),
                    p if p < 0.05 => ('.', 33),
                    _ => return None,
                };
                Some(Mark { ch, strength })
            }
        }
    }
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
fn hash(seed: u64, x: i64, y: i64) -> u64 {
    let mut z = seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// `hash` as a number from 0 to 1.
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
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
            |b: &Backdrop| -> Vec<_> { (0..2000).map(|i| b.mark(i % 80, i / 80)).collect() };
        let a = Backdrop::new(Kind::Terrain, 7);
        assert_eq!(marks(&a), marks(&Backdrop::new(Kind::Terrain, 7)));
        assert_ne!(marks(&a), marks(&Backdrop::new(Kind::Terrain, 8)));
        // The terrain: only its characters, 6% to 16% bright.
        for mark in marks(&a).into_iter().flatten() {
            assert!(RAMP.contains(&(mark.ch as u8)));
            assert!((15..=41).contains(&mark.strength));
        }
    }

    #[test]
    fn stars_are_few() {
        let stars = Backdrop::new(Kind::Stars, 42);
        let lit = (0..10_000)
            .filter(|i| stars.mark(i % 100, i / 100).is_some())
            .count();
        // About 5% of the cells.
        assert!((200..900).contains(&lit), "{lit}");
    }
}
