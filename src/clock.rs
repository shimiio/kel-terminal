//! The day and the time on the wall, for a moment in the past.
//!
//! Rust knows a moment only as seconds since 1970 in Greenwich. What the
//! clock here showed then (the time zone, summer time) the system knows:
//! Windows by two calls of its own, Linux and macOS by C's `localtime_r`.
//! No crate for it: it's one line of text, under a tab's output from last
//! time (see `Terminal::show_last_time`).

use std::time::{SystemTime, UNIX_EPOCH};

/// `time` as `14 Nov 18:32`, here. `None` if the system can't say.
pub fn day_and_time(time: SystemTime) -> Option<String> {
    let seconds = time.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let (month, day, hour, minute) = local(seconds as i64)?;
    text(month, day, hour, minute)
}

/// `month` from 1.
fn text(month: u32, day: u32, hour: u32, minute: u32) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS.get(month.checked_sub(1)? as usize)?;
    Some(format!("{day} {month} {hour:02}:{minute:02}"))
}

/// (month from 1, day, hour, minute) here, `seconds` after 1970.
#[cfg(windows)]
fn local(seconds: i64) -> Option<(u32, u32, u32, u32)> {
    /// Windows' `SYSTEMTIME`, 8 numbers: year, month (from 1), day of the
    /// week, day, hour, minute, second, millisecond.
    type Parts = [u16; 8];
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FileTimeToSystemTime(time: *const u64, parts: *mut Parts) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const u8,
            utc: *const Parts,
            local: *mut Parts,
        ) -> i32;
    }
    // Windows counts from 1601, in steps of 100 ns: 11 644 473 600 seconds
    // more, 10 000 000 steps a second.
    let steps = u64::try_from(seconds.checked_add(11_644_473_600)?).ok()? * 10_000_000;
    let (mut utc, mut here) = ([0; 8], [0; 8]);
    // Safety: each call reads and fills exactly the values it's given. A null
    // `zone`: the time zone set now, with its summer time as it was then.
    let ok = unsafe {
        FileTimeToSystemTime(&steps, &mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut here) != 0
    };
    let [_, month, _, day, hour, minute, ..] = here.map(u32::from);
    ok.then_some((month, day, hour, minute))
}

/// (month from 1, day, hour, minute) here, `seconds` after 1970.
#[cfg(not(windows))]
fn local(seconds: i64) -> Option<(u32, u32, u32, u32)> {
    use std::ffi::{c_char, c_int, c_long};
    /// C's `struct tm` as Linux and macOS have it. The first 9: second,
    /// minute, hour, day, month (from 0), year, day of the week, of the year,
    /// summer time.
    #[repr(C)]
    struct Tm {
        parts: [c_int; 9],
        _offset: c_long,
        _zone: *const c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const c_long, tm: *mut Tm) -> *mut Tm;
    }
    let time = c_long::try_from(seconds).ok()?;
    let mut tm = Tm {
        parts: [0; 9],
        _offset: 0,
        _zone: std::ptr::null(),
    };
    // Safety: it reads `time` and fills `tm`, both alive here; null if it can't.
    let ok = unsafe { !localtime_r(&time, &mut tm).is_null() };
    let [_, minute, hour, day, month, ..] = tm.parts;
    let part = |n: c_int| u32::try_from(n).ok();
    ok.then_some((part(month)? + 1, part(day)?, part(hour)?, part(minute)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_and_a_time() {
        assert_eq!(text(11, 14, 18, 32).as_deref(), Some("14 Nov 18:32"));
        assert_eq!(text(1, 2, 7, 5).as_deref(), Some("2 Jan 07:05"));
        assert_eq!(text(0, 1, 0, 0), None);
        assert_eq!(text(13, 1, 0, 0), None);
    }

    #[test]
    fn the_system_knows_the_time_here() {
        // Whatever the zone: a day of a month, an hour, a minute.
        let (month, day, hour, minute) = local(1_700_000_000).expect("a time");
        assert!((1..=12).contains(&month) && (1..=31).contains(&day));
        assert!(hour < 24 && minute < 60);
        // 14 Nov 2023 22:13 in Greenwich: the 14th or the 15th anywhere.
        assert!(month == 11 && (14..=15).contains(&day), "{month} {day}");
        assert!(day_and_time(SystemTime::now()).is_some());
    }
}
