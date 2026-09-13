use crate::proto;
use crop::Rope;
use std::mem;

pub(crate) fn apply_document_changes(
    file_contents: &Rope,
    mut content_changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
) -> Rope {
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
        return text;
    }

    for change in content_changes {
        // SAFETY: we already handled the `None` case above
        let range = proto::text_range(&text, change.range.unwrap());
        text.replace(range, &change.text);
    }

    text
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
    for pair in changes.windows(2) {
        if pair[1].range?.end >= pair[0].range?.start {
            return None;
        }
    }
    let index = proto::LspPositionIndex::new(text);
    let mut ranges = Vec::with_capacity(changes.len());
    for change in changes {
        let lsp_range = change.range?;
        let range = index.checked_text_range(lsp_range)?;
        // Checked conversion clamps oversized columns to the line end, unlike the sequential
        // conversion. Only reuse positions that resolve exactly without that clamping.
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
    use crate::utils::{apply_document_changes, rope_eq_str};
    use crop::Rope;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    #[test]
    fn rope_eq_str_compares_chunked_utf8() {
        let rope = Rope::from("alpha\nβeta😀");
        assert!(rope_eq_str(&rope, "alpha\nβeta😀"));
        assert!(!rope_eq_str(&rope, "alpha\nβeta"));
        assert!(!rope_eq_str(&rope, "alpha\nβeta😃"));
    }

    #[test]
    fn descending_document_changes_preserve_oversized_columns() {
        let changes = [
            (Range::new(Position::new(2, 0), Position::new(2, 1)), "X"),
            (Range::new(Position::new(0, 4), Position::new(0, 5)), "Y"),
        ]
        .map(|(range, text)| TextDocumentContentChangeEvent {
            range: Some(range),
            range_length: None,
            text: text.into(),
        });
        let text = apply_document_changes(&Rope::from("abc\ndef\nghi"), changes.into());
        assert_eq!(text, "abc\nYef\nXhi");
    }

    #[test]
    fn descending_document_changes_preserve_unicode_and_line_endings() {
        let original = Rope::from("a😀b\r\ncéc\rd𐐀e\nfgh");
        let changes = [
            (Range::new(Position::new(3, 1), Position::new(3, 2)), "😀\r\nmore"),
            (Range::new(Position::new(2, 1), Position::new(2, 3)), "世\n界"),
            (Range::new(Position::new(1, 1), Position::new(1, 2)), "E\r"),
            (Range::new(Position::new(0, 1), Position::new(0, 3)), "🙂"),
        ]
        .map(|(range, text)| TextDocumentContentChangeEvent {
            range: Some(range),
            range_length: None,
            text: text.into(),
        });
        let sequential = changes.iter().fold(original.clone(), |text, change| {
            apply_document_changes(&text, vec![change.clone()])
        });
        let text = apply_document_changes(&original, changes.into());
        assert_eq!(text, sequential);
        assert_eq!(text, "a🙂b\r\ncE\rc\rd世\n界e\nf😀\r\nmoreh");
    }

    #[test]
    fn test_apply_document_changes() {
        macro_rules! c {
            [$($sl:expr, $sc:expr; $el:expr, $ec:expr => $text:expr),+] => {
                vec![$(TextDocumentContentChangeEvent {
                    range: Some(Range {
                        start: Position { line: $sl, character: $sc },
                        end: Position { line: $el, character: $ec },
                    }),
                    range_length: None,
                    text: String::from($text),
                }),+]
            };
        }

        let text = apply_document_changes(&Rope::new(), vec![]);
        assert_eq!(text, "");

        let text = apply_document_changes(
            &text,
            vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: String::from("the"),
            }],
        );
        assert_eq!(text, "the");

        let text = apply_document_changes(&text, c![0, 3; 0, 3 => " quick"]);
        assert_eq!(text, "the quick");

        let text = apply_document_changes(&text, c![0, 0; 0, 4 => "", 0, 5; 0, 5 => " foxes"]);
        assert_eq!(text, "quick foxes");

        let text = apply_document_changes(&text, c![0, 11; 0, 11 => "\ndream"]);
        assert_eq!(text, "quick foxes\ndream");

        let text = apply_document_changes(&text, c![1, 0; 1, 0 => "have "]);
        assert_eq!(text, "quick foxes\nhave dream");

        let text = apply_document_changes(
            &text,
            c![0, 0; 0, 0 => "the ", 1, 4; 1, 4 => " quiet", 1, 16; 1, 16 => "s\n"],
        );
        assert_eq!(text, "the quick foxes\nhave quiet dreams\n");

        let text = apply_document_changes(&text, c![0, 15; 0, 15 => "\n", 2, 17; 2, 17 => "\n"]);
        assert_eq!(text, "the quick foxes\n\nhave quiet dreams\n\n");

        let text = apply_document_changes(
            &text,
            c![1, 0; 1, 0 => "DREAM", 2, 0; 2, 0 => "they ", 3, 0; 3, 0 => "DON'T THEY?"],
        );
        assert_eq!(text, "the quick foxes\nDREAM\nthey have quiet dreams\nDON'T THEY?\n");

        let text = apply_document_changes(&text, c![0, 10; 1, 5 => "", 2, 0; 2, 12 => ""]);
        assert_eq!(text, "the quick \nthey have quiet dreams\n");

        let text = Rope::from("❤️");
        let text = apply_document_changes(&text, c![0, 0; 0, 0 => "a"]);
        assert_eq!(text, "a❤️");

        let text = Rope::from("a\nb");
        let text = apply_document_changes(&text, c![0, 1; 1, 0 => "\nțc", 0, 1; 1, 1 => "d"]);
        assert_eq!(text, "adcb");

        let text = Rope::from("a\nb");
        let text = apply_document_changes(&text, c![0, 1; 1, 0 => "ț\nc", 0, 2; 0, 2 => "c"]);
        assert_eq!(text, "ațc\ncb");

        let text = Rope::from("function increment() public {\n    // 中文😀\n    umber++;\n}");
        let text = apply_document_changes(&text, c![2, 4; 2, 9 => "number"]);
        assert_eq!(text, "function increment() public {\n    // 中文😀\n    number++;\n}");
    }
}
