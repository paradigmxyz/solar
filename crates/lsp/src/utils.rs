use crate::proto;
use crop::Rope;
use std::mem;

/// Applies sequential changes atomically, rejecting malformed ranges without changing the input.
pub(crate) fn apply_document_changes(
    file_contents: &Rope,
    mut content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
) -> Option<Rope> {
    // If at least one of the changes is a full document change, use the last
    // of them as the starting point and ignore all previous changes.
    let (mut text, content_changes) =
        match content_changes.iter().rposition(|change| change.range.is_none()) {
            Some(idx) => {
                let text = Rope::from(mem::take(&mut content_changes[idx].text));
                (text, &content_changes[idx + 1..])
            }
            None => (file_contents.clone(), &content_changes[..]),
        };

    if let Some(ranges) = descending_ranges(&text, content_changes) {
        // Edits sorted from the end of the document toward the beginning do not change the byte
        // offsets of any later range. Resolve every UTF-16 range against one position index.
        for (range, change) in ranges.into_iter().zip(content_changes) {
            text.replace(range, &change.text);
        }
        return Some(text);
    }

    for change in content_changes {
        // SAFETY: we already handled the `None` case above
        let range = proto::text_range(&text, change.range.unwrap())?;
        text.replace(range, &change.text);
    }

    Some(text)
}

fn descending_ranges(
    text: &Rope,
    changes: &[lsp_types::TextDocumentContentChangeEvent],
) -> Option<Vec<std::ops::Range<usize>>> {
    if changes.len() < 2 {
        return None;
    }
    // Reject sequential and adjacent edits before indexing the document. An insertion at the
    // boundary of a neighboring edit can observe that edit's newly inserted text.
    if !changes.is_sorted_by(|a, b| a.range.zip(b.range).is_some_and(|(a, b)| b.end < a.start)) {
        return None;
    }
    let index = proto::LspPositionIndex::new(text);
    let mut ranges = Vec::with_capacity(changes.len());
    for change in changes {
        let lsp_range = change.range?;
        let range = index.checked_text_range(lsp_range)?;
        // Clamping can make apparently separate edits touch. Keep these edits sequential so
        // later positions resolve against any newly inserted text at that boundary.
        if index.position_at_byte(range.start) != Some(lsp_range.start)
            || index.position_at_byte(range.end) != Some(lsp_range.end)
        {
            return None;
        }
        ranges.push(range);
    }
    Some(ranges)
}

/// Compare a rope's UTF-8 bytes with a contiguous string without flattening the rope.
#[inline]
pub(crate) fn rope_eq_str(contents: &Rope, text: &str) -> bool {
    if contents.byte_len() != text.len() {
        return false;
    }

    let mut offset = 0;
    for chunk in contents.chunks() {
        let end = offset + chunk.len();
        if text.get(offset..end) != Some(chunk) {
            return false;
        }
        offset = end;
    }
    true
}

pub(crate) fn rope_to_string(rope: &Rope) -> String {
    let mut source = String::with_capacity(rope.byte_len());
    for chunk in rope.chunks() {
        source.push_str(chunk);
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    fn edits(edits: &[(u32, u32, u32, u32, &str)]) -> Vec<TextDocumentContentChangeEvent> {
        edits
            .iter()
            .map(|&(start_line, start, end_line, end, text)| TextDocumentContentChangeEvent {
                range: Some(Range::new(
                    Position::new(start_line, start),
                    Position::new(end_line, end),
                )),
                range_length: None,
                text: text.into(),
            })
            .collect()
    }

    #[test]
    fn rope_eq_str_compares_chunked_utf8() {
        let rope = Rope::from("alpha\nβeta😀");
        assert!(rope_eq_str(&rope, "alpha\nβeta😀"));
        assert!(!rope_eq_str(&rope, "alpha\nβeta"));
        assert!(!rope_eq_str(&rope, "alpha\nβeta😃"));
    }

    #[test]
    fn descending_document_changes_match_sequential_edits() {
        let changes = edits(&[(2, 0, 2, 1, "X"), (0, 4, 0, 5, "Y")]);
        let text = apply_document_changes(&Rope::from("abc\ndef\nghi"), changes).unwrap();
        assert_eq!(text, "abcY\ndef\nXhi");

        let original = Rope::from("a😀b\r\ncéc\rd𐐀e\nfgh");
        let changes = edits(&[
            (3, 1, 3, 2, "😀\r\nmore"),
            (2, 1, 2, 3, "世\n界"),
            (1, 1, 1, 2, "E\r"),
            (0, 1, 0, 3, "🙂"),
        ]);
        let sequential = changes.iter().fold(original.clone(), |text, change| {
            apply_document_changes(&text, vec![change.clone()]).unwrap()
        });
        let text = apply_document_changes(&original, changes).unwrap();
        assert_eq!(text, sequential);
        assert_eq!(text, "a🙂b\r\ncE\rc\rd世\n界e\nf😀\r\nmoreh");
    }

    #[test]
    fn test_apply_document_changes() {
        let full = |text: &str| {
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.into(),
            }]
        };
        let mut text = Rope::new();
        for (changes, expected) in [
            (vec![], ""),
            (full("the"), "the"),
            (edits(&[(0, 3, 0, 3, " quick")]), "the quick"),
            (edits(&[(0, 0, 0, 4, ""), (0, 5, 0, 5, " foxes")]), "quick foxes"),
            (edits(&[(0, 11, 0, 11, "\ndream")]), "quick foxes\ndream"),
            (edits(&[(1, 0, 1, 0, "have ")]), "quick foxes\nhave dream"),
            (
                edits(&[(0, 0, 0, 0, "the "), (1, 4, 1, 4, " quiet"), (1, 16, 1, 16, "s\n")]),
                "the quick foxes\nhave quiet dreams\n",
            ),
            (
                edits(&[(0, 15, 0, 15, "\n"), (2, 17, 2, 17, "\n")]),
                "the quick foxes\n\nhave quiet dreams\n\n",
            ),
            (
                edits(&[(1, 0, 1, 0, "DREAM"), (2, 0, 2, 0, "they "), (3, 0, 3, 0, "DON'T THEY?")]),
                "the quick foxes\nDREAM\nthey have quiet dreams\nDON'T THEY?\n",
            ),
            (edits(&[(0, 10, 1, 5, ""), (2, 0, 3, 0, "")]), "the quick \nthey have quiet dreams\n"),
        ] {
            text = apply_document_changes(&text, changes).unwrap();
            assert_eq!(text, expected);
        }

        for (original, changes, expected) in [
            ("❤️", edits(&[(0, 0, 0, 0, "a")]), "a❤️"),
            ("a\nb", edits(&[(0, 1, 1, 0, "\nțc"), (0, 1, 1, 1, "d")]), "adcb"),
            ("a\nb", edits(&[(0, 1, 1, 0, "ț\nc"), (0, 2, 0, 2, "c")]), "ațc\ncb"),
            (
                "function increment() public {\n    // 中文😀\n    umber++;\n}",
                edits(&[(2, 4, 2, 9, "number")]),
                "function increment() public {\n    // 中文😀\n    number++;\n}",
            ),
        ] {
            assert_eq!(apply_document_changes(&Rope::from(original), changes).unwrap(), expected);
        }
    }
}
