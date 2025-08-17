use std::mem;

use crop::Rope;

use crate::proto;

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

    for change in content_changes {
        // SAFETY: we already handled the `None` case above
        let range = proto::text_range(&text, change.range.unwrap());
        text.replace(range, &change.text);
    }

    text
}

#[cfg(test)]
mod tests {
    use crop::Rope;
    use lsp_types::{Position, Range, TextDocumentContentChangeEvent};

    use crate::utils::apply_document_changes;

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
    }
}
