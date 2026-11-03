//! Printing of the `;` comment after a block label, shared by the IR text formats.

use std::fmt;

/// The column at which the comment after a block label starts, counting from the start of the line.
///
/// This is the column LLVM's `AsmWriter` pads `; preds = ...` to.
pub const LABEL_COMMENT_COLUMN: usize = 50;

/// Displays a block label line: `label`, then, if there is a `comment`, a `;` aligned like LLVM's
/// `formatted_raw_ostream::PadToColumn(50)`.
///
/// The `;` goes to [`LABEL_COMMENT_COLUMN`], or one space after the label when the label reaches
/// that column. `label` is the whole text before the comment, so that its width counts the
/// indentation.
pub fn display_label_comment(
    label: impl fmt::Display,
    comment: Option<impl fmt::Display>,
) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        let label = label.to_string();
        f.write_str(&label)?;
        let Some(comment) = &comment else { return Ok(()) };
        let padding = LABEL_COMMENT_COLUMN.saturating_sub(label.chars().count()).max(1);
        write!(f, "{:padding$}; {comment}", "")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_to_the_comment_column() {
        let text = display_label_comment("  bb1:", Some("preds = bb0")).to_string();
        assert_eq!(text.find(';'), Some(LABEL_COMMENT_COLUMN));
        assert!(text.ends_with("; preds = bb0"));
    }

    #[test]
    fn keeps_one_space_after_a_long_label() {
        let label = "x".repeat(LABEL_COMMENT_COLUMN);
        assert_eq!(display_label_comment(&label, Some("c")).to_string(), format!("{label} ; c"));
        let label = "x".repeat(LABEL_COMMENT_COLUMN - 1);
        assert_eq!(display_label_comment(&label, Some("c")).to_string(), format!("{label} ; c"));
        let label = "x".repeat(LABEL_COMMENT_COLUMN - 2);
        assert_eq!(display_label_comment(&label, Some("c")).to_string(), format!("{label}  ; c"));
    }

    #[test]
    fn omits_a_missing_comment() {
        assert_eq!(display_label_comment("bb1:", None::<&str>).to_string(), "bb1:");
    }
}
