//! Independently validate explicit MIR placement/link inputs. Image extents are
//! bounds-checked, constructor ranges refer to existing memory, and relocations
//! retain their own widths. Nothing is resolved by compiling the MIR caller.

use super::super::Input;
use alloy_primitives::U256;
use serde::Deserialize;
use solar_mir_interpreter::PlacementRead;
use std::collections::BTreeMap;

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Layout {
    pub(super) code: Option<Vec<u8>>,
    functions: BTreeMap<String, Function>,
    constructor: Option<Range>,
    data: BTreeMap<usize, Range>,
    immutable_slots: BTreeMap<usize, u64>,
    immutable_values: BTreeMap<usize, String>,
    libraries: BTreeMap<usize, String>,
    gas_before: Option<u64>,
    allocations: BTreeMap<String, BTreeMap<usize, Allocation>>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Allocation {
    Static { address: u64, size: u64 },
    Heap { size: u64 },
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Function {
    heap_floor: Option<u64>,
    frame_base: Option<u64>,
    return_buffer: Option<u64>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Range {
    offset: u64,
    size: u64,
}

impl Layout {
    pub(super) fn parse(input: &Input) -> Result<Self, String> {
        let Some(value) = &input.mir_layout else {
            return Ok(Self::default());
        };
        for group in ["data", "immutable_slots", "immutable_values", "libraries"] {
            if let Some(table) = value.get(group).and_then(serde_json::Value::as_object) {
                for key in table.keys() {
                    if key.parse::<u32>().ok().is_none_or(|n| n.to_string() != *key) {
                        return Err("invalid MIR layout binding key".into());
                    }
                }
            }
        }
        if let Some(allocations) = value.get("allocations").and_then(serde_json::Value::as_object) {
            for sites in allocations.values().filter_map(serde_json::Value::as_object) {
                for key in sites.keys() {
                    if key.parse::<usize>().ok().is_none_or(|n| n.to_string() != *key) {
                        return Err("invalid MIR allocation placement key".into());
                    }
                }
            }
        }
        let layout: Self = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        if layout.code.as_ref().is_some_and(|c| c.len() > 65536)
            || layout.functions.len() > 128
            || layout.data.len() > 128
            || layout.libraries.len() > 128
            || layout.immutable_slots.len() > 128
            || layout.immutable_values.len() > 128
        {
            return Err("MIR layout input limit".into());
        }
        if layout.allocations.len() > 128
            || layout.allocations.values().map(BTreeMap::len).sum::<usize>() > 4096
        {
            return Err("MIR allocation placement limit".into());
        }
        for (name, sites) in &layout.allocations {
            if name.len() > 256
                || !name.is_ascii()
                || name.chars().any(char::is_control)
                || sites.keys().any(|&site| site >= 4096)
            {
                return Err("invalid MIR allocation placement key".into());
            }
            for placement in sites.values() {
                let (address, size) = match *placement {
                    Allocation::Static { address, size } => (Some(address), size),
                    Allocation::Heap { size } => (None, size),
                };
                if size > 65536
                    || address.is_some_and(|address| {
                        address < 128
                            || !address.is_multiple_of(32)
                            || address.checked_add(size).is_none()
                    })
                {
                    return Err("invalid deferred allocation placement".into());
                }
            }
        }
        for (name, function) in &layout.functions {
            if name.len() > 256 || !name.is_ascii() || name.chars().any(char::is_control) {
                return Err("invalid MIR placement function name".into());
            }
            for address in [function.heap_floor, function.frame_base, function.return_buffer]
                .into_iter()
                .flatten()
            {
                if address < 128 || !address.is_multiple_of(32) {
                    return Err("MIR placements must be aligned and at least 128".into());
                }
            }
        }
        let inside = |range: &Range, length: u64| {
            range.offset.checked_add(range.size).is_some_and(|end| end <= length)
        };
        if layout.constructor.as_ref().is_some_and(|r| !inside(r, input.memory.size)) {
            return Err("constructor range is outside initial memory".into());
        }
        for range in layout.data.values() {
            let code = layout.code.as_ref().ok_or("linked data requires a code image")?;
            if !inside(range, code.len() as u64) {
                return Err("linked data is outside the code image".into());
            }
        }
        for (&id, &slot) in &layout.immutable_slots {
            if !(128..=65504).contains(&slot)
                || !slot.is_multiple_of(32)
                || layout.immutable_values.contains_key(&id)
            {
                return Err("invalid or ambiguous immutable staging slot".into());
            }
        }
        for value in layout.immutable_values.values() {
            U256::from_str_radix(value, 10).map_err(|e| e.to_string())?;
        }
        for value in layout.libraries.values() {
            if U256::from_str_radix(value, 10).map_err(|e| e.to_string())? >> 160 != U256::ZERO {
                return Err("library address exceeds 160 bits".into());
            }
        }
        if layout.gas_before.is_some_and(|g| g > 30000000) {
            return Err("MIR gas window exceeds 30000000".into());
        }
        let mut pointer = [0u8; 32];
        for (index, byte) in pointer.iter_mut().enumerate() {
            *byte = input.memory.data.get(64 + index).copied().unwrap_or_default();
        }
        let pointer = U256::from_be_bytes(pointer);
        if layout
            .functions
            .values()
            .any(|f| f.heap_floor.is_some_and(|floor| U256::from(floor) > pointer))
        {
            return Err("heap floor exceeds the explicit initial FMP".into());
        }
        Ok(layout)
    }

    pub(super) fn read(&self, query: PlacementRead<'_>) -> Option<U256> {
        Some(match query {
            PlacementRead::HeapFloor(name) => U256::from(self.functions.get(name)?.heap_floor?),
            PlacementRead::FrameBase(name) => U256::from(self.functions.get(name)?.frame_base?),
            PlacementRead::ReturnBuffer(name) => {
                U256::from(self.functions.get(name)?.return_buffer?)
            }
            PlacementRead::AllocationAddress(name, site) => {
                match self.allocations.get(name)?.get(&site)? {
                    Allocation::Static { address, .. } => U256::from(*address),
                    Allocation::Heap { .. } => return None,
                }
            }
            PlacementRead::AllocationSize(name, site) => {
                match self.allocations.get(name)?.get(&site)? {
                    Allocation::Static { size, .. } | Allocation::Heap { size } => {
                        U256::from(*size)
                    }
                }
            }
            PlacementRead::ConstructorBase => U256::from(self.constructor.as_ref()?.offset),
            PlacementRead::ConstructorEnd => {
                let r = self.constructor.as_ref()?;
                U256::from(r.offset + r.size)
            }
            PlacementRead::DataSize(id) => U256::from(self.data.get(&id)?.size),
            PlacementRead::ImmutableSlot(id) => U256::from(*self.immutable_slots.get(&id)?),
            PlacementRead::ImmutableValue(id) => {
                U256::from_str_radix(self.immutable_values.get(&id)?, 10).ok()?
            }
            PlacementRead::Library(id) => {
                U256::from_str_radix(self.libraries.get(&id)?, 10).ok()?
            }
            PlacementRead::GasBefore => U256::from(self.gas_before?),
        })
    }
}
