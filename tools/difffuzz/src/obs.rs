//! What a run observably did, and how two runs are compared.

use alloy_primitives::{U256, hex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Status {
    #[default]
    Success,
    Revert,
    /// An exceptional halt, with the reason.
    Halt(String),
    /// The leg cannot run the call, with the reason; never compared.
    Unsupported(String),
}

/// One leg's observation of one transaction.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Obs {
    pub(crate) status: Status,
    /// Return data or revert data.
    #[serde(with = "hexbytes")]
    pub(crate) output: Vec<u8>,
    /// Logs as topics and data; empty unless the run succeeded.
    pub(crate) logs: Vec<(Vec<U256>, Vec<u8>)>,
    /// Slots the run wrote, with their final values (interpreters; the EVM reports changed slots).
    pub(crate) writes: BTreeMap<U256, U256>,
    /// The contract's whole nonzero storage after the run, where the leg knows it.
    pub(crate) storage: BTreeMap<U256, U256>,
}

mod hexbytes {
    use alloy_primitives::hex;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode_prefixed(v))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(s).map_err(serde::de::Error::custom)
    }
}

impl Obs {
    pub(crate) fn unsupported(reason: impl Into<String>) -> Self {
        Self { status: Status::Unsupported(reason.into()), ..Self::default() }
    }

    pub(crate) fn is_unsupported(&self) -> bool {
        matches!(self.status, Status::Unsupported(_))
    }

    pub(crate) fn describe(&self) -> String {
        let status = match &self.status {
            Status::Success => "success".to_string(),
            Status::Revert => "revert".to_string(),
            Status::Halt(r) => format!("halt ({r})"),
            Status::Unsupported(r) => format!("unsupported ({r})"),
        };
        format!("{status} {}", hex::encode_prefixed(&self.output))
    }
}

/// Compares two EVM runs of different compilers: status, output, logs and the whole storage.
/// Halts compare by kind only, since their reasons name opcodes.
pub(crate) fn compare_full(a: &Obs, b: &Obs) -> Vec<String> {
    let mut diffs = vec![];
    let kind = |s: &Status| std::mem::discriminant(s);
    if kind(&a.status) != kind(&b.status) || a.output != b.output {
        diffs.push(format!("outcome: {} vs {}", a.describe(), b.describe()));
        return diffs;
    }
    if a.logs != b.logs {
        diffs.push(format!("logs: {} vs {}", logs(&a.logs), logs(&b.logs)));
    }
    if a.storage != b.storage {
        diffs.push(format!("storage: {} vs {}", slots(&a.storage), slots(&b.storage)));
    }
    diffs
}

/// Compares an interpreter's run (`b`) with the EVM's run of the same compiled program (`a`),
/// whose storage before the call was `before`.
pub(crate) fn compare_interp(a: &Obs, b: &Obs, before: &BTreeMap<U256, U256>) -> Vec<String> {
    let mut diffs = vec![];
    let kind = |s: &Status| std::mem::discriminant(s);
    if kind(&a.status) != kind(&b.status) || a.output != b.output {
        diffs.push(format!("outcome: {} vs {}", a.describe(), b.describe()));
        return diffs;
    }
    if a.status != Status::Success {
        return diffs;
    }
    if a.logs != b.logs {
        diffs.push(format!("logs: {} vs {}", logs(&a.logs), logs(&b.logs)));
    }
    for (slot, value) in &b.writes {
        let evm = a.storage.get(slot).copied().unwrap_or_default();
        if evm != *value {
            diffs.push(format!("slot {slot:#x}: {evm:#x} vs {value:#x}"));
        }
    }
    for (slot, value) in &a.storage {
        let old = before.get(slot).copied().unwrap_or_default();
        if old != *value && !b.writes.contains_key(slot) {
            diffs.push(format!("slot {slot:#x}: EVM wrote {value:#x}, interpreter did not"));
        }
    }
    for slot in before.keys() {
        if !a.storage.contains_key(slot) && !b.writes.contains_key(slot) {
            diffs.push(format!("slot {slot:#x}: EVM cleared it, interpreter did not"));
        }
    }
    diffs
}

fn logs(l: &[(Vec<U256>, Vec<u8>)]) -> String {
    let v: Vec<_> = l
        .iter()
        .map(|(t, d)| {
            let t: Vec<_> = t.iter().map(|t| format!("{t:#x}")).collect();
            format!("[{}] {}", t.join(","), hex::encode_prefixed(d))
        })
        .collect();
    format!("[{}]", v.join("; "))
}

fn slots(s: &BTreeMap<U256, U256>) -> String {
    let v: Vec<_> = s.iter().map(|(k, v)| format!("{k:#x}={v:#x}")).collect();
    format!("{{{}}}", v.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreter_writes_must_match() {
        let before = BTreeMap::from([(U256::from(1), U256::from(5))]);
        let evm =
            Obs { storage: BTreeMap::from([(U256::from(2), U256::from(7))]), ..Obs::default() };
        // The EVM wrote slot 2 and cleared slot 1.
        let both = Obs {
            writes: BTreeMap::from([(U256::from(1), U256::ZERO), (U256::from(2), U256::from(7))]),
            ..Obs::default()
        };
        assert!(compare_interp(&evm, &both, &before).is_empty());
        let missed =
            Obs { writes: BTreeMap::from([(U256::from(2), U256::from(7))]), ..Obs::default() };
        assert_eq!(compare_interp(&evm, &missed, &before).len(), 1);
        let wrong = Obs {
            writes: BTreeMap::from([(U256::from(1), U256::ZERO), (U256::from(2), U256::from(8))]),
            ..Obs::default()
        };
        assert_eq!(compare_interp(&evm, &wrong, &before).len(), 1);
    }

    #[test]
    fn outcomes_compare_byte_for_byte() {
        let a = Obs { status: Status::Revert, output: vec![0x4e, 0x48], ..Obs::default() };
        let b = Obs { status: Status::Revert, output: vec![0x4e, 0x49], ..Obs::default() };
        assert_eq!(compare_full(&a, &b).len(), 1);
        assert!(compare_full(&a, &a).is_empty());
    }
}
