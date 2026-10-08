//! What `-Zllm-optimize=live` prints while it works.
//!
//! Every conversation reports on stderr as it goes: the turn it starts, the model's reasoning and
//! reply as they stream in, what the turn used, and the verdict on each candidate. Each line names
//! its module and function, so conversations that run at once interleave by whole lines: streamed
//! text waits for the end of its line, or for the model to switch between reasoning and reply.
//! Reasoning is marked `┆` and the reply `│`. Control characters and the marks that reorder text
//! are printed as escapes, so a reply cannot steer the terminal. Nothing is printed when
//! diagnostics are machine-readable.

use super::wire::Delta;
use std::{borrow::Cow, fmt, io::Write, sync::Mutex};

/// One conversation's lines on stderr.
pub(super) struct Voice {
    /// Whether anything is printed.
    enabled: bool,
    /// The module and function the conversation is about.
    tag: String,
    /// Streamed text whose line has not ended, and whether it is reasoning.
    pending: Mutex<(String, bool)>,
}

impl Voice {
    pub(super) fn new(enabled: bool, module: &str, function: &str) -> Self {
        Self { enabled, tag: format!("{module} @{function}"), pending: Mutex::default() }
    }

    /// Reports what the conversation does.
    pub(super) fn say(&self, message: impl fmt::Display) {
        if self.enabled {
            self.flush();
            print_line(&format!("llm-optimize {}: {message}", self.tag));
        }
    }

    /// Prints streamed text a line at a time.
    pub(super) fn stream(&self, delta: &Delta) {
        if !self.enabled {
            return;
        }
        let (text, reasoning) = match delta {
            Delta::Reasoning(text) => (text, true),
            Delta::Reply(text) => (text, false),
        };
        let mut pending = self.pending.lock().unwrap();
        if pending.1 != reasoning && !pending.0.is_empty() {
            // The model switched between reasoning and reply mid-line.
            let line = std::mem::take(&mut pending.0);
            self.print_streamed(&line, pending.1);
        }
        pending.1 = reasoning;
        pending.0.push_str(text);
        while let Some(end) = pending.0.find('\n') {
            let line = pending.0.drain(..=end).collect::<String>();
            self.print_streamed(line.trim_end_matches(['\n', '\r']), reasoning);
        }
    }

    /// Prints what is left of the streamed text.
    pub(super) fn flush(&self) {
        if !self.enabled {
            return;
        }
        let mut pending = self.pending.lock().unwrap();
        if !pending.0.is_empty() {
            let line = std::mem::take(&mut pending.0);
            self.print_streamed(&line, pending.1);
        }
    }

    fn print_streamed(&self, line: &str, reasoning: bool) {
        let marker = if reasoning { '┆' } else { '│' };
        print_line(&format!("  {} {marker} {line}", self.tag));
    }
}

/// Writes `line` to stderr in one piece, so concurrent lines never mix, with what a terminal would
/// act on escaped: lines quote the model's replies, and a reply must not be able to move the
/// cursor, change colors, or hide text.
fn print_line(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{}", printable(line));
}

/// Returns `text` with every character a terminal acts on rather than shows written as its
/// escape: the control characters other than tabs, and the marks that reorder text around them.
fn printable(text: &str) -> Cow<'_, str> {
    let acts = |c: char| {
        (c.is_control() && c != '\t')
            || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    };
    if !text.chars().any(acts) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if acts(c) {
            escaped.extend(c.escape_unicode());
        } else {
            escaped.push(c);
        }
    }
    Cow::Owned(escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_voice_prints_nothing() {
        let voice = Voice::new(false, "C", "f");
        voice.stream(&Delta::Reply("text\n".into()));
        voice.say("event");
        assert!(voice.pending.lock().unwrap().0.is_empty());
    }

    #[test]
    fn lines_wait_for_their_end() {
        let voice = Voice::new(true, "C", "f");
        voice.stream(&Delta::Reasoning("half".into()));
        assert_eq!(*voice.pending.lock().unwrap(), ("half".to_string(), true));
        // Switching to the reply prints the reasoning's unfinished line first.
        voice.stream(&Delta::Reply("one\ntwo".into()));
        assert_eq!(*voice.pending.lock().unwrap(), ("two".to_string(), false));
        voice.flush();
        assert!(voice.pending.lock().unwrap().0.is_empty());
    }

    #[test]
    fn terminal_controls_are_escaped() {
        assert!(matches!(printable("plain\ttext │ ┆"), Cow::Borrowed("plain\ttext │ ┆")));
        assert_eq!(
            printable("ok\x1b[2J\x1b]0;title\x07\r\u{9b}31m\u{202e}desrever\u{2066}"),
            "ok\\u{1b}[2J\\u{1b}]0;title\\u{7}\\u{d}\\u{9b}31m\\u{202e}desrever\\u{2066}"
        );
    }
}
