//! Links in the output: `https://...` in the text, underlined while Ctrl is held
//! over it, opened in the browser with Ctrl+click.
//!
//! Only the text is looked at, one line at a time (`Grid::link_at` joins a line
//! cut at the window's edge). A link starts with `http://` or `https://` and goes
//! on to a space, a quote or an angle bracket. What ends a sentence isn't part of
//! it:
//!
//! | In the terminal | The link |
//! |---|---|
//! | `see https://a.io/x.` | `https://a.io/x` |
//! | `(https://a.io/x)` | `https://a.io/x` |
//! | `https://a.io/wiki/Rust_(language)` | all of it: the `(` `)` pair up |
//! | `https://a.io/@50.45,30.52,12z` | all of it: the commas are inside |
//!
//! Only `http(s)` in the text: a `file://` link opened by Windows could start a
//! program, and anything can write text into the terminal (a log, a page `curl`
//! fetched).
//!
//! A program can also mark text as a link (OSC 8): `ls --hyperlink`'s names,
//! delta's files. Its address isn't on the screen, so it can say anything: it
//! goes through `target`, which opens a web page or shows a file in Explorer,
//! and runs nothing.

use std::ops::Range;
use std::path::PathBuf;

/// What a link opens (see `target`).
#[derive(Debug, PartialEq)]
pub enum Target {
    /// A web page, in the browser.
    Web,
    /// A file or a folder on this computer: shown in Explorer, never run.
    File(PathBuf),
}

/// What `url` opens, if Kel opens it: `http(s)` in the browser; `file://` on
/// this computer in Explorer. A file on another computer (`file://server/...`)
/// isn't: Windows would log in to it to look, with your name. Anything else
/// (`mailto:`, an app's own) neither.
pub fn target(url: &str) -> Option<Target> {
    let lower = url.to_ascii_lowercase();
    if SCHEMES.iter().any(|scheme| lower.starts_with(scheme)) {
        return Some(Target::Web);
    }
    let rest = url.get(7..).filter(|_| lower.starts_with("file://"))?;
    // The computer's name, then the path: `file://pc/C:/x`, `file:///C:/x`.
    let slash = rest.find('/')?;
    let host = &rest[..slash];
    let this = std::env::var("COMPUTERNAME").unwrap_or_default();
    let here = host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case(&this);
    if !here {
        return None;
    }
    let path = percent_decode(&rest[slash..]);
    // `/C:/Users` → `C:\Users`.
    let path = path
        .strip_prefix('/')
        .filter(|p| p.get(1..2) == Some(":"))
        .unwrap_or(&path);
    Some(Target::File(PathBuf::from(path.replace('/', "\\"))))
}

/// `%20` → a space: a URL writes some characters as `%` and their hex number.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex
            .filter(|_| bytes[i] == b'%')
            .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The beginnings of a link, in small letters (`HTTPS://` is one too).
const SCHEMES: [&str; 2] = ["https://", "http://"];

/// The links in a line's letters: where each one is in `text`, left to right.
pub fn find_urls(text: &[char]) -> Vec<Range<usize>> {
    let mut urls = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let Some(scheme) = scheme_at(text, i) else {
            i += 1;
            continue;
        };
        let rest = i + scheme;
        let mut end = rest;
        while end < text.len() && in_url(text[end]) {
            end += 1;
        }
        let end = trim_end(text, rest, end);
        if end > rest {
            urls.push(i..end);
            i = end;
        } else {
            i = rest; // "https://" and nothing after it
        }
    }
    urls
}

/// The length of the `http://` or `https://` at `text[i]`, if one is there.
fn scheme_at(text: &[char], i: usize) -> Option<usize> {
    SCHEMES.iter().find_map(|scheme| {
        let n = scheme.len();
        let here = text.get(i..i + n)?;
        let same = here
            .iter()
            .zip(scheme.chars())
            .all(|(a, b)| a.to_ascii_lowercase() == b);
        same.then_some(n)
    })
}

/// Whether `c` can be part of a link. Not: spaces, the quotes and brackets that
/// go around one (`"..."`, `<...>`, `` `...` ``), and the lines of boxes that
/// programs draw (`│`): a link in a box ends at its side.
fn in_url(c: char) -> bool {
    let around = matches!(c, '"' | '\'' | '<' | '>' | '`');
    let box_line = ('\u{2500}'..='\u{259f}').contains(&c);
    !(c.is_whitespace() || c.is_control() || around || box_line)
}

/// Where the link `text[start..end]` really ends: without the punctuation of the
/// sentence around it. Only at the end: `.` or `,` inside a link stays. A closing
/// bracket stays when the link has its opening one: `Rust_(language)`.
fn trim_end(text: &[char], start: usize, mut end: usize) -> usize {
    while end > start {
        let url = &text[start..end];
        let count = |c: char| url.iter().filter(|&&x| x == c).count();
        let unpaired = |open: char, close: char| count(open) < count(close);
        let drop = match url[url.len() - 1] {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' => unpaired('(', ')'),
            ']' => unpaired('[', ']'),
            '}' => unpaired('{', '}'),
            _ => false,
        };
        if !drop {
            break;
        }
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_link_opens() {
        assert_eq!(target("https://a.io/x"), Some(Target::Web));
        assert_eq!(target("HTTP://a.io"), Some(Target::Web));
        let file = |p: &str| Some(Target::File(PathBuf::from(p)));
        assert_eq!(
            target("file:///C:/Users/me/a%20b.txt"),
            file(r"C:\Users\me\a b.txt")
        );
        assert_eq!(target("file://localhost/C:/x"), file(r"C:\x"));
        // Another computer's file, or something else: not opened.
        assert_eq!(target("file://evil.example/share/x"), None);
        assert_eq!(target("mailto:me@a.io"), None);
        assert_eq!(target("javascript:alert(1)"), None);
        assert_eq!(target("file:"), None);
    }

    /// The links in `line`, as text.
    fn urls(line: &str) -> Vec<String> {
        let text: Vec<char> = line.chars().collect();
        find_urls(&text)
            .into_iter()
            .map(|r| text[r].iter().collect())
            .collect()
    }

    #[test]
    fn a_link_in_a_sentence() {
        assert_eq!(urls("see https://a.io/x for more"), ["https://a.io/x"]);
        assert_eq!(
            urls("http://a.io and HTTPS://B.IO"),
            ["http://a.io", "HTTPS://B.IO"]
        );
        assert!(urls("no links here, not even https:// alone").is_empty());
    }

    #[test]
    fn the_sentence_end_isnt_part_of_it() {
        assert_eq!(urls("see https://a.io/x."), ["https://a.io/x"]);
        assert_eq!(
            urls("https://a.io/pull/12, merged"),
            ["https://a.io/pull/12"]
        );
        assert_eq!(urls("really? https://a.io/x?!"), ["https://a.io/x"]);
        // Inside, the same marks stay.
        assert_eq!(
            urls("at https://a.io/@50.45,30.52,12z"),
            ["https://a.io/@50.45,30.52,12z"]
        );
        assert_eq!(urls("https://a.io/?q=1&b=2"), ["https://a.io/?q=1&b=2"]);
    }

    #[test]
    fn brackets_pair_up() {
        assert_eq!(urls("(https://a.io/x)"), ["https://a.io/x"]);
        assert_eq!(
            urls("https://a.io/wiki/Rust_(language)"),
            ["https://a.io/wiki/Rust_(language)"]
        );
        assert_eq!(urls("[docs](https://docs.rs/vte)"), ["https://docs.rs/vte"]);
        assert_eq!(
            urls("(see https://a.io/wiki/A_(b))."),
            ["https://a.io/wiki/A_(b)"]
        );
    }

    #[test]
    fn quotes_and_boxes_end_it() {
        assert_eq!(urls("url = \"https://a.io/x\""), ["https://a.io/x"]);
        assert_eq!(urls("'https://a.io/x'"), ["https://a.io/x"]);
        assert_eq!(urls("<https://a.io/x>"), ["https://a.io/x"]);
        assert_eq!(urls("│ https://a.io/x│"), ["https://a.io/x"]);
    }
}
