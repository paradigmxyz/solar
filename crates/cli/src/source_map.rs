//! Legacy Solidity instruction source maps.

use solar_codegen::backend::evm::{DebugFunctionExit, DebugInstruction};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::Gcx;
use std::fmt::Write as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceMapEntry {
    start: i64,
    length: i64,
    source: i64,
    jump: char,
    modifier_depth: i64,
}

impl SourceMapEntry {
    const INITIAL: Self =
        Self { start: -1, length: -1, source: -1, jump: '\0', modifier_depth: -1 };
}

/// Encoder for Solidity's legacy `s:l:f:j:m` instruction source maps.
pub(crate) struct SourceMapEncoder {
    source_ids: FxHashMap<u32, i64>,
}

impl SourceMapEncoder {
    /// Creates an encoder for the compilation's Standard JSON source IDs.
    pub(crate) fn new(gcx: Gcx<'_>) -> Self {
        let source_ids = gcx
            .hir
            .source_ids()
            .map(|id| (gcx.hir.source(id).file.start_pos.0, id.index() as i64))
            .collect();
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
            .map(|instruction| instruction.offset as usize)
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
        function_entries: &FxHashSet<usize>,
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
            let source_id = *self.source_ids.get(&source.file.start_pos.0)?;
            Some((source.data.start as i64, source.data.len() as i64, source_id))
        });
        let (start, length, source) = location.unwrap_or((-1, -1, -1));
        // `i` denotes an internal transfer and is meaningful only on a jump.
        // `o` also covers RETURN, which is the external function's terminal transfer.
        let is_jump = matches!(instruction.opcode, 0x56 | 0x57);
        let enters_function = instruction.function_invoke.is_some()
            || static_jump_target(bytecode, previous, instruction)
                .is_some_and(|target| function_entries.contains(&target));
        let jump = if is_jump && enters_function {
            'i'
        } else if instruction.function_exit == Some(DebugFunctionExit::Return) {
            'o'
        } else {
            '-'
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
    if !matches!(instruction.opcode, 0x56 | 0x57) {
        return None;
    }
    let previous = previous?;
    let width = previous.opcode.checked_sub(0x5f)? as usize;
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
    (bytecode.get(target).copied() == Some(0x5b)).then_some(target)
}

fn encode(entries: impl IntoIterator<Item = SourceMapEntry>) -> String {
    let mut output = String::new();
    let mut previous = SourceMapEntry::INITIAL;

    for (index, entry) in entries.into_iter().enumerate() {
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
                write!(output, "{}", entry.start).unwrap();
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.length != previous.length {
                write!(output, "{}", entry.length).unwrap();
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.source != previous.source {
                write!(output, "{}", entry.source).unwrap();
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.jump != previous.jump {
                output.push(entry.jump);
            }
            components -= 1;
        }
        if components > 0 {
            output.push(':');
            if entry.modifier_depth != previous.modifier_depth {
                write!(output, "{}", entry.modifier_depth).unwrap();
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
        let base = SourceMapEntry { start: 1, length: 2, source: 0, jump: '-', modifier_depth: 0 };
        let length = SourceMapEntry { length: 3, ..base };
        let invoke = SourceMapEntry { jump: 'i', ..length };
        let modifier = SourceMapEntry { modifier_depth: 1, ..invoke };

        assert_eq!(encode([base, base, length, invoke, modifier]), "1:2:0:-:0;;:3;:::i;::::1");
    }

    #[test]
    fn encodes_missing_source_location() {
        let entry = SourceMapEntry { jump: '-', modifier_depth: 0, ..SourceMapEntry::INITIAL };
        assert_eq!(encode([entry]), ":::-:0");
    }
}
