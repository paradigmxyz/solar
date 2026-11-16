//! Recognizing the jump of a `break` or `continue` statement from its source origin.

use crate::mir::InstructionMetadata;
use solar_sema::Gcx;

/// Returns whether `metadata` is the origin of a `break` or `continue` statement.
///
/// A loop condition desugars to `if (cond) body else break`, whose `break` takes the span of the
/// loop, so only the text of the span tells a source statement from the desugared jump.
pub(crate) fn is_loop_jump(gcx: Gcx<'_>, metadata: &InstructionMetadata) -> bool {
    let files = gcx.sess.source_map().files();
    metadata.source_spans().any(|span| {
        let Ok(source) = files.span_to_snippet(span) else { return false };
        let word = source.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'));
        matches!(word.into_iter().next(), Some("break" | "continue"))
    })
}
