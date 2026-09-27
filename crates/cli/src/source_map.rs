//! Legacy Solidity instruction source maps.

use core::fmt::NumBuffer;
use solar_codegen::backend::evm::{DebugFunctionExit, DebugInstruction, op};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::BytePos;
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
    pub(crate) fn encode(
        &self,
        gcx: Gcx<'_>,
        bytecode: &[u8],
        instructions: &[DebugInstruction],
    ) -> String {
        let function_entries = instructions
            .iter()
            .filter(|instruction| instruction.function_invoke.is_some())
            .map(|instruction| instruction.offset)
            .collect::<FxHashSet<_>>();
        let entries = instructions.iter().enumerate().map(|(index, instruction)| {
            self.entry(
                gcx,
                bytecode,
                &function_entries,
                instructions.get(index.wrapping_sub(1)),
                instruction,
            )
        });
        encode(entries)
    }

    fn entry(
        &self,
        gcx: Gcx<'_>,
        bytecode: &[u8],
        function_entries: &FxHashSet<u32>,
        previous: Option<&DebugInstruction>,
        instruction: &DebugInstruction,
    ) -> SourceMapEntry {
        // A shared instruction has no single source origin in this format. Its
        // incoming transfers retain path-specific locations where available;
        // choosing one origin here would attribute other callers to an unrelated
        // source statement. This applies to sharing in both MIR and EVM IR.
        // NOTE: An incoming transfer may be optimized into a zero-byte fallthrough.
        // Its checkpoint is then unavailable; keep the shared location unknown
        // (-1, -1, -1) rather than changing codegen to manufacture a source stop.
        let location = match instruction.source_spans.as_slice() {
            [span] => Some(*span),
            _ => None,
        }
        .and_then(|span| {
            let source = gcx.sess.source_map().span_to_source(span).ok()?;
            let source_id = *self.source_ids.get(&source.file.start_pos)?;
            Some((source.data.start as i64, source.data.len() as i64, source_id.index() as i64))
        });
        let (start, length, source) = location.unwrap_or((-1, -1, -1));
        // Legacy `i`/`o` markers describe internal jumps, not external returns.
        let is_jump = matches!(instruction.opcode, op::JUMP | op::JUMPI);
        let enters_function = instruction.function_invoke.is_some()
            || static_jump_target(bytecode, previous, instruction)
                .and_then(|target| u32::try_from(target).ok())
                .is_some_and(|target| function_entries.contains(&target));
        let jump = if is_jump && enters_function {
            b'i'
        } else if is_jump && instruction.function_exit == Some(DebugFunctionExit::Return) {
            b'o'
        } else {
            b'-'
        };

        SourceMapEntry {
            start,
            length,
            source,
            jump,
            modifier_depth: i64::from(instruction.modifier_depth),
        }
    }
}

/// Returns the statically encoded destination of a jump preceded by `PUSH`.
pub(crate) fn static_jump_target(
    bytecode: &[u8],
    previous: Option<&DebugInstruction>,
    instruction: &DebugInstruction,
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
}
