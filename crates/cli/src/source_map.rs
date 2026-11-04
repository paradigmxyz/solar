//! Legacy Solidity instruction source maps.

use core::fmt::NumBuffer;
use solar_codegen::backend::evm::{DebugFunctionExit, DebugInfo, DebugInstruction, op};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{BytePos, Span, source_map::SourceMapFiles};
use solar_sema::{Gcx, hir::SourceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceMapEntry {
    start: i64,
    length: i64,
    source: i64,
    jump: u8,
    modifier_depth: i64,
}

impl SourceMapEntry {
    const INITIAL: Self = Self { start: -1, length: -1, source: -1, jump: 0, modifier_depth: -1 };
}

/// Encoder for Solidity's legacy `s:l:f:j:m` instruction source maps.
pub(crate) struct SourceMapEncoder {
    source_ids: FxHashMap<BytePos, SourceId>,
}

impl SourceMapEncoder {
    /// Creates an encoder for the compilation's source IDs.
    pub(crate) fn new(gcx: Gcx<'_>) -> Self {
        let source_ids =
            gcx.hir.source_ids().map(|id| (gcx.hir.source(id).file.start_pos, id)).collect();
        Self { source_ids }
    }

    /// Encodes final EVM instructions.
    pub(crate) fn encode(&self, gcx: Gcx<'_>, bytecode: &[u8], debug_info: &DebugInfo) -> String {
        let function_entries = debug_info
            .instructions(bytecode)
            .filter(|instruction| instruction.location.function_invoke.is_some())
            .map(|instruction| instruction.offset)
            .collect::<FxHashSet<_>>();
        let files = gcx.sess.source_map().files();
        let mut previous = None;
        let entries = debug_info.instructions(bytecode).map(|instruction| {
            let entry =
                self.entry(&files, bytecode, &function_entries, previous.as_ref(), &instruction);
            previous = Some(instruction);
            entry
        });
        encode(entries)
    }

    fn entry(
        &self,
        files: &SourceMapFiles<'_>,
        bytecode: &[u8],
        function_entries: &FxHashSet<u32>,
        previous: Option<&DebugInstruction<'_>>,
        instruction: &DebugInstruction<'_>,
    ) -> SourceMapEntry {
        // The format holds one origin per instruction. Origins that form one nesting chain
        // (each pair is nested within one source file) map to the first span, the
        // instruction's own location: origin lists keep insertion order and merges append, so
        // a nested origin added from either side never changes the map. Any other set is
        // shared between unrelated source statements: its incoming transfers retain
        // path-specific locations where available, and choosing one origin here would
        // attribute other callers to an unrelated statement. This applies to sharing in both
        // MIR and EVM IR.
        // NOTE: An incoming transfer may be optimized into a zero-byte fallthrough.
        // Its checkpoint is then unavailable; keep such a shared location unknown
        // (-1, -1, -1) rather than changing codegen to manufacture a source stop.
        let location = first_nested_span(instruction.location.source_spans).and_then(|span| {
            let source = files.span_to_source(span).ok()?;
            let source_id = *self.source_ids.get(&source.file.start_pos)?;
            Some((source.data.start as i64, source.data.len() as i64, source_id.index() as i64))
        });
        let (start, length, source) = location.unwrap_or((-1, -1, -1));
        // Legacy `i`/`o` markers describe internal jumps, not external returns.
        let is_jump = matches!(instruction.opcode, op::JUMP | op::JUMPI);
        let enters_function = instruction.location.function_invoke.is_some()
            || static_jump_target(bytecode, previous, instruction)
                .and_then(|target| u32::try_from(target).ok())
                .is_some_and(|target| function_entries.contains(&target));
        let jump = if is_jump && enters_function {
            b'i'
        } else if is_jump && instruction.location.function_exit == Some(DebugFunctionExit::Return) {
            b'o'
        } else {
            b'-'
        };

        SourceMapEntry {
            start,
            length,
            source,
            jump,
            modifier_depth: i64::from(instruction.location.modifier_depth),
        }
    }
}

/// Returns the first of `spans` when they form one nesting chain.
///
/// Spans of different source files occupy disjoint position ranges, so they never nest.
fn first_nested_span(spans: &[Span]) -> Option<Span> {
    let nested = spans.iter().all(|&a| spans.iter().all(|&b| a.contains(b) || b.contains(a)));
    spans.first().copied().filter(|_| nested)
}

/// Returns the statically encoded destination of a jump preceded by `PUSH`.
pub(crate) fn static_jump_target(
    bytecode: &[u8],
    previous: Option<&DebugInstruction<'_>>,
    instruction: &DebugInstruction<'_>,
) -> Option<usize> {
    if !matches!(instruction.opcode, op::JUMP | op::JUMPI) {
        return None;
    }
    let previous = previous?;
    let width = previous.opcode.checked_sub(op::PUSH0)? as usize;
    if !(1..=32).contains(&width)
        || previous.offset as usize + width + 1 != instruction.offset as usize
    {
        return None;
    }
    let start = previous.offset as usize + 1;
    let mut target = 0usize;
    for &byte in bytecode.get(start..instruction.offset as usize)? {
        target = target.checked_mul(256)?.checked_add(usize::from(byte))?;
    }
    (bytecode.get(target).copied() == Some(op::JUMPDEST)).then_some(target)
}

fn encode(entries: impl IntoIterator<Item = SourceMapEntry>) -> String {
    let entries = entries.into_iter();
    let mut output = String::with_capacity(entries.size_hint().0.saturating_mul(8));
    let mut buffer = NumBuffer::new();
    let mut previous = SourceMapEntry::INITIAL;

    for (index, entry) in entries.enumerate() {
        if index != 0 {
            output.push(';');
        }

        let mut components = 5;
        if entry.modifier_depth == previous.modifier_depth {
            components -= 1;
            if entry.jump == previous.jump {
                components -= 1;
                if entry.source == previous.source {
                    components -= 1;
                    if entry.length == previous.length {
                        components -= 1;
                        if entry.start == previous.start {
                            components -= 1;
                        }
                    }
                }
            }
        }

        if components > 0 {
            if entry.start != previous.start {
                output.push_str(entry.start.format_into(&mut buffer));
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.length != previous.length {
                output.push_str(entry.length.format_into(&mut buffer));
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.source != previous.source {
                output.push_str(entry.source.format_into(&mut buffer));
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.jump != previous.jump {
                output.push(char::from(entry.jump));
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.modifier_depth != previous.modifier_depth {
                output.push_str(entry.modifier_depth.format_into(&mut buffer));
            }
        }

        previous = entry;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};
    use solar_codegen::backend::evm::DebugLocation;
    use solar_interface::source_map::SourceMap;
    use std::path::PathBuf;

    #[test]
    fn compresses_unchanged_fields() {
        let base = SourceMapEntry { start: 1, length: 2, source: 0, jump: b'-', modifier_depth: 0 };
        let length = SourceMapEntry { length: 3, ..base };
        let invoke = SourceMapEntry { jump: b'i', ..length };
        let modifier = SourceMapEntry { modifier_depth: 1, ..invoke };

        assert_eq!(encode([base, base, length, invoke, modifier]), "1:2:0:-:0;;:3;:::i;::::1");
    }

    #[test]
    fn encodes_missing_source_location() {
        let entry = SourceMapEntry { jump: b'-', modifier_depth: 0, ..SourceMapEntry::INITIAL };
        assert_eq!(encode([entry]), ":::-:0");
    }

    #[test]
    fn maps_nested_origins_to_the_first_span() {
        let source_map = SourceMap::default();
        let file = source_map.new_source_file(PathBuf::from("a.sol"), "x".repeat(100)).unwrap();
        let encoder = SourceMapEncoder {
            source_ids: FxHashMap::from_iter([(file.start_pos, SourceId::from_usize(0))]),
        };
        let files = source_map.files();
        let span = |lo, hi| Span::new(BytePos(lo), BytePos(hi));
        let encode_spans = |source_spans: &[Span]| {
            let instruction = DebugInstruction {
                offset: 0,
                opcode: op::ADD,
                location: DebugLocation { source_spans, ..Default::default() },
            };
            let entry =
                encoder.entry(&files, &[op::ADD], &FxHashSet::default(), None, &instruction);
            encode([entry])
        };

        let outer = span(10, 50);
        let inner = span(20, 30);
        let innermost = span(22, 25);
        let overlapping = span(40, 60);

        // A single span.
        assert_data_eq!(encode_spans(&[inner]), str!["20:10:0:-:0"]);
        // A nested pair writes its first span, whether that is the outer or the inner one.
        assert_data_eq!(encode_spans(&[outer, inner]), str!["10:40:0:-:0"]);
        assert_data_eq!(encode_spans(&[inner, outer]), str!["20:10:0:-:0"]);
        // A chain of three, and a duplicate.
        assert_data_eq!(encode_spans(&[inner, outer, innermost]), str!["20:10:0:-:0"]);
        assert_data_eq!(encode_spans(&[inner, inner]), str!["20:10:0:-:0"]);
        // A pair that overlaps without nesting, and a disjoint pair.
        assert_data_eq!(encode_spans(&[outer, overlapping]), str![":::-:0"]);
        assert_data_eq!(encode_spans(&[inner, span(60, 70)]), str![":::-:0"]);
        // Both enclose a span but not each other.
        assert_data_eq!(encode_spans(&[outer, overlapping, span(42, 48)]), str![":::-:0"]);
        // No span.
        assert_data_eq!(encode_spans(&[]), str![":::-:0"]);
    }
}
