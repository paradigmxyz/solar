use alloy_primitives::{Bytes, Keccak256};
use serde::{Serialize, Serializer};
use solar_codegen::LibraryReference;
use std::fmt;

const PLACEHOLDER_BYTE_LEN: usize = 20;

/// Bytecode serialized as hex with textual placeholders for unresolved libraries.
#[derive(Clone, Debug, Default)]
pub(crate) struct MaybeHexBytecode(MaybeHexBytecodeInner);

#[derive(Clone, Debug, Default)]
struct MaybeHexBytecodeInner {
    bytecode: Bytes,
    placeholders: Box<[Placeholder]>,
}

impl MaybeHexBytecode {
    pub(crate) fn new(bytecode: Bytes, references: &[LibraryReference]) -> Self {
        let mut placeholders = references.iter().map(Placeholder::new).collect::<Vec<_>>();
        placeholders.sort_unstable_by_key(|placeholder| placeholder.start);
        Self(MaybeHexBytecodeInner { bytecode, placeholders: placeholders.into_boxed_slice() })
    }
}

impl fmt::Display for MaybeHexBytecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut start = 0;
        for placeholder in &self.0.placeholders {
            write!(
                f,
                "{}",
                alloy_primitives::hex::display(&self.0.bytecode[start..placeholder.start])
            )?;
            f.write_str(str::from_utf8(&placeholder.text).map_err(|_| fmt::Error)?)?;
            start = placeholder.start + PLACEHOLDER_BYTE_LEN;
        }
        write!(f, "{}", alloy_primitives::hex::display(&self.0.bytecode[start..]))
    }
}

impl Serialize for MaybeHexBytecode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

#[derive(Clone, Debug)]
struct Placeholder {
    start: usize,
    text: [u8; 40],
}

impl Placeholder {
    fn new(reference: &LibraryReference) -> Self {
        let mut hasher = Keccak256::new();
        hasher.update(reference.source.as_bytes());
        hasher.update(b":");
        hasher.update(reference.name.as_bytes());
        let hash = hasher.finalize();

        let mut text = [0; 40];
        text[..3].copy_from_slice(b"__$");
        alloy_primitives::hex::encode_to_slice(&hash[..17], &mut text[3..37]).unwrap();
        text[37..].copy_from_slice(b"$__");
        Self { start: reference.start, text }
    }
}
