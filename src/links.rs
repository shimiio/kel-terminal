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
//! Only `http(s)`: a `file://` link opened by Windows could start a program, and
//! anything can write text into the terminal (a log, a page `curl` fetched).

use std::ops::Range;

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
