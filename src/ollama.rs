//! Ollama (<https://ollama.com>): a program that runs language models on this
//! computer and answers on a port of its own. Kel asks it for a command (see `ask`).
//!
//! No crate for it: Ollama speaks plain HTTP here, no encryption, so a request
//! is a few lines written to a socket, and from the JSON that comes back one
//! string is taken.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Nothing listening is known this soon.
const CONNECT: Duration = Duration::from_secs(2);
/// A model read from the disk for the first time takes a while (5 GB: ~15 s),
/// and one that doesn't fit the graphics card answers slowly.
const ANSWER: Duration = Duration::from_secs(120);

/// The model's answer to `messages`, each a role (`system`, `user`, `assistant`)
/// and its text. `Err`: why there's none, in a few words, to be shown.
pub fn chat(address: &str, model: &str, messages: &[(&str, &str)]) -> Result<String, String> {
    let messages: Vec<String> = messages
        .iter()
        .map(|(role, content)| {
            format!(
                "{{\"role\":{},\"content\":{}}}",
                quoted(role),
                quoted(content)
            )
        })
        .collect();
    // `think`: a model that thinks first spends hundreds of words on it, a
    // minute for one line. `temperature` 0: the likeliest answer, the same each
    // time. `num_predict`: a command is short; a model that rambles is cut.
    let body = format!(
        "{{\"model\":{},\"stream\":false,\"think\":false,\
         \"options\":{{\"temperature\":0,\"num_predict\":200}},\"messages\":[{}]}}",
        quoted(model),
        messages.join(","),
    );
    let answer = post(address, "/api/chat", &body)?;
    string(&answer, "content").ok_or_else(|| "Ollama's answer has no text".to_string())
}

/// Reads `model` into memory, so the first question doesn't wait for it: a
/// chat without messages does only that. What comes back isn't looked at: a
/// model that's missing is said when the question is asked.
pub fn load(address: &str, model: &str) {
    let body = format!("{{\"model\":{},\"messages\":[]}}", quoted(model));
    let _ = post(address, "/api/chat", &body);
}

/// The body of Ollama's answer to a POST of `body` (JSON) to `path`.
/// HTTP/1.0: the answer comes whole, and its end is the connection's.
fn post(address: &str, path: &str, body: &str) -> Result<String, String> {
    let nobody = || format!("no Ollama at {address}: is it running?");
    let to = address
        .to_socket_addrs()
        .ok()
        .and_then(|mut found| found.next())
        .ok_or_else(nobody)?;
    let mut stream = TcpStream::connect_timeout(&to, CONNECT).map_err(|_| nobody())?;
    let _ = stream.set_read_timeout(Some(ANSWER));
    let request = format!(
        "POST {path} HTTP/1.0\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len(),
    );
    let failed = |error: std::io::Error| match error.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => "Ollama took too long".to_string(),
        _ => format!("Ollama: {error}"),
    };
    stream.write_all(request.as_bytes()).map_err(failed)?;
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).map_err(failed)?;
    let answer = String::from_utf8_lossy(&answer);
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .ok_or("Ollama's answer isn't HTTP")?;
    let status = head.lines().next().unwrap_or_default();
    if status.split(' ').nth(1) == Some("200") {
        return Ok(body.to_string());
    }
    // Its own words: `model "llama9" not found, try pulling it first`.
    Err(string(body, "error").unwrap_or_else(|| format!("Ollama: {status}")))
}

/// `text` as a JSON string, its quotes included.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// The string at the first `"key":"` in a JSON text, its escapes undone. Not a
/// JSON parser: enough for an answer whose shape is known.
fn string(json: &str, key: &str) -> Option<String> {
    let mark = format!("\"{key}\":\"");
    let start = json.find(&mark)? + mark.len();
    let mut out = String::new();
    let mut chars = json[start..].chars();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                // Ollama (Go) writes `<`, `>` and `&` this way.
                'u' => {
                    let code: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&code, 16).ok()?;
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                other => out.push(other), // \" \\ \/
            },
            ch => out.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    /// Something that answers one request as Ollama would, with `status` and
    /// `body`: its address, and the request's body once it came.
    fn server(status: &str, body: &str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let answer = format!("HTTP/1.0 {status}\r\nContent-Type: application/json\r\n\r\n{body}");
        let asked = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            // The head, then as many bytes of body as it says.
            let body = loop {
                let n = stream.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .map_or(0, |n| n.parse().unwrap());
                    if body.len() >= length {
                        break body.to_string();
                    }
                }
            };
            stream.write_all(answer.as_bytes()).unwrap();
            body
        });
        (address, asked)
    }

    #[test]
    fn an_answer() {
        let body = r#"{"model":"m","message":{"role":"assistant","content":"git log \u003e a.txt"},"done":true}"#;
        let (address, asked) = server("200 OK", body);
        let messages = [
            ("system", "Be \"short\"."),
            ("user", "the log\ninto a file"),
        ];
        assert_eq!(
            chat(&address, "m", &messages).as_deref(),
            Ok("git log > a.txt")
        );
        let asked = asked.join().unwrap();
        assert!(asked.contains(r#""model":"m""#), "{asked}");
        assert!(asked.contains(r#""think":false"#), "{asked}");
        assert!(
            asked.contains(r#"{"role":"system","content":"Be \"short\"."}"#),
            "{asked}"
        );
        assert!(
            asked.contains(r#""content":"the log\ninto a file""#),
            "{asked}"
        );
    }

    #[test]
    fn ollama_says_why_not() {
        let body = r#"{"error":"model \"llama9\" not found, try pulling it first"}"#;
        let (address, _) = server("404 Not Found", body);
        assert_eq!(
            chat(&address, "llama9", &[("user", "hi")]),
            Err("model \"llama9\" not found, try pulling it first".to_string())
        );
        // Nothing of its own to say: the status.
        let (address, _) = server("500 Internal Server Error", "");
        assert_eq!(
            chat(&address, "m", &[("user", "hi")]),
            Err("Ollama: HTTP/1.0 500 Internal Server Error".to_string())
        );
    }

    #[test]
    fn nobody_there() {
        // A port that was free a moment ago.
        let address = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().to_string()
        };
        let why = chat(&address, "m", &[("user", "hi")]).unwrap_err();
        assert_eq!(why, format!("no Ollama at {address}: is it running?"));
        let why = chat("not an address", "m", &[]).unwrap_err();
        assert!(why.starts_with("no Ollama at not an address"), "{why}");
    }

    #[test]
    fn json_strings() {
        assert_eq!(quoted("a \"b\" c:\\d\ne\t"), r#""a \"b\" c:\\d\ne\u0009""#);
        let json = r#"{"a":"x","content":"say \"hi\" \\ \u0026 go\nnow","b":"y"}"#;
        assert_eq!(
            string(json, "content").as_deref(),
            Some("say \"hi\" \\ & go\nnow")
        );
        assert_eq!(string(json, "missing"), None);
        assert_eq!(string(r#"{"content":"cut off"#, "content"), None);
    }
}
