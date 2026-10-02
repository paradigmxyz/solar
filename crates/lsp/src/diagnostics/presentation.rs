//! Shared presentation for native compiler diagnostics and external diagnostic formats.
//! Primary-location details stay in the message; other valid locations remain navigable.
//! Missing locations fall back to text, and tags use known diagnostic codes, never message guesses.

use lsp_types::{DiagnosticRelatedInformation, DiagnosticTag, Location};

pub(crate) struct DiagnosticMessage {
    primary: Location,
    pub(crate) message: String,
    pub(crate) related_information: Vec<DiagnosticRelatedInformation>,
}

impl DiagnosticMessage {
    pub(crate) fn new(primary: Location, message: String) -> Self {
        Self { primary, message, related_information: Vec::new() }
    }

    pub(crate) fn push(&mut self, location: Option<Location>, message: &str) {
        if message.is_empty() {
            return;
        }
        if let Some(location) = location
            && location != self.primary
        {
            if !self
                .related_information
                .iter()
                .any(|related| related.location == location && related.message == message)
            {
                self.related_information
                    .push(DiagnosticRelatedInformation { location, message: message.to_owned() });
            }
        } else {
            append_message(&mut self.message, message);
        }
    }
}

/// Appends one complete detail unless it is already present at newline boundaries.
pub(crate) fn append_message(message: &mut String, detail: &str) {
    if detail.is_empty()
        || message.match_indices(detail).any(|(start, _)| {
            let end = start + detail.len();
            (start == 0 || message.as_bytes()[start - 1] == b'\n')
                && (end == message.len() || message.as_bytes()[end] == b'\n')
        })
    {
        return;
    }
    if !message.is_empty() {
        message.push('\n');
    }
    message.push_str(detail);
}

pub(crate) fn solidity_diagnostic_tags(code: Option<&str>) -> Option<Vec<DiagnosticTag>> {
    match code? {
        "8417" => Some(vec![DiagnosticTag::DEPRECATED]),
        "2072" | "5667" => Some(vec![DiagnosticTag::UNNECESSARY]),
        _ => None,
    }
}

pub(crate) fn forge_diagnostic_tags(code: Option<&str>) -> Option<Vec<DiagnosticTag>> {
    match code {
        Some("unused-import") => Some(vec![DiagnosticTag::UNNECESSARY]),
        _ => solidity_diagnostic_tags(code),
    }
}
