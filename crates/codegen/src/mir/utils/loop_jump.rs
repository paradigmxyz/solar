//! Recognizing the jump of a `break` or `continue` statement from its source origin.

use crate::mir::InstructionMetadata;
use solar_sema::Gcx;

/// Returns whether `metadata` is the origin of a `break` or `continue` statement.
///
/// A loop condition desugars to `if (cond) body else break`, whose `break` takes the span of the
/// loop, so only the text of the span tells a source statement from the desugared jump.
pub(crate) fn is_loop_jump(gcx: Gcx<'_>, metadata: &InstructionMetadata) -> bool {
    metadata.source_spans().any(|span| {
        let Ok(source) = gcx.sess.source_map().span_to_snippet(span) else { return false };
        ["break", "continue"].iter().any(|keyword| {
            source.strip_prefix(keyword).is_some_and(|rest| {
                !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$')
            })
        })
    })
}
