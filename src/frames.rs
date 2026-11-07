//! `KEL_FRAMES=1` in the environment: how long Kel's frames take, on stderr,
//! once a second. For a slowness on a system you can't look at yourself:
//!
//! ```sh
//! KEL_FRAMES=1 /Applications/Kel.app/Contents/MacOS/kel
//! ```

use std::time::{Duration, Instant};

/// How often a line is written, if there were frames.
const EVERY: Duration = Duration::from_secs(1);

/// The frames since the last line. `None`: not asked for, nothing is counted.
pub struct FrameLog(Option<Frames>);

struct Frames {
    count: u32,
    total: Duration,
    worst: Duration,
    since: Instant,
}

impl FrameLog {
    pub fn from_env() -> Self {
        let on = std::env::var_os("KEL_FRAMES").is_some();
        FrameLog(on.then(|| Frames {
            count: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
            since: Instant::now(),
        }))
    }

    /// One frame took `took`.
    pub fn add(&mut self, took: Duration) {
        let Some(frames) = &mut self.0 else {
            return;
        };
        frames.count += 1;
        frames.total += took;
        frames.worst = frames.worst.max(took);
        if frames.since.elapsed() >= EVERY {
            eprintln!("{}", line(frames.count, frames.total, frames.worst));
            frames.count = 0;
            frames.total = Duration::ZERO;
            frames.worst = Duration::ZERO;
            frames.since = Instant::now();
        }
    }
}

/// `count` frames that took `total`, the longest `worst`, as a line.
fn line(count: u32, total: Duration, worst: Duration) -> String {
    let ms = |time: Duration| time.as_secs_f64() * 1000.0;
    let average = ms(total) / f64::from(count.max(1));
    format!(
        "{count} frames, {average:.1} ms each, the longest {:.1} ms",
        ms(worst)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_frames() {
        let ms = Duration::from_millis;
        assert_eq!(
            line(4, ms(40), ms(25)),
            "4 frames, 10.0 ms each, the longest 25.0 ms"
        );
        assert_eq!(
            line(0, ms(0), ms(0)),
            "0 frames, 0.0 ms each, the longest 0.0 ms"
        );
    }
}
