//! What `-Zllm-optimize=live` prints while it works.
//!
//! Every conversation reports on stderr as it goes: the turn it starts, the model's reasoning and
//! reply as they stream in, what the turn used, and the verdict on each candidate. Each line names
//! its module and function, so conversations that run at once interleave by whole lines: streamed
//! text waits for the end of its line, or for the model to switch between reasoning and reply.
//! Reasoning is marked `┆` and the reply `│`. Nothing is printed when diagnostics are
//! machine-readable.

use super::wire::Delta;
use std::{fmt, io::Write, sync::Mutex};

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

/// Writes `line` to stderr in one piece, so concurrent lines never mix.
fn print_line(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
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
}
