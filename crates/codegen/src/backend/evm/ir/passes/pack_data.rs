//! Pack referenced program data and reuse contained byte strings.

use super::EvmPass;
use crate::backend::evm::ir::{DataId, DataRef, Module, PushValue};
use alloy_primitives::Bytes;
use memchr::memmem;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;

pub(super) struct PackData;

impl EvmPass for PackData {
    fn name(&self) -> &'static str {
        "pack-data"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        pack_data(module)
    }
}

fn pack_data(module: &mut Module) -> bool {
    let mut referenced = DenseBitSet::new_empty(module.data.len());
    for block in &module.blocks {
        for inst in &block.instructions {
            if let Some(PushValue::Data(data)) = inst.value {
                referenced.insert(data.id);
            }
        }
    }
    let mut referenced = referenced.iter().collect::<Vec<_>>();
    referenced.sort_unstable_by(|&a, &b| {
        module.data[b].len().cmp(&module.data[a].len()).then_with(|| a.cmp(&b))
    });

    let mut packed = IndexVec::<DataId, Bytes>::new();
    let mut remap = FxHashMap::default();
    for old_id in referenced {
        let data = &module.data[old_id];
        let contained = packed.iter_enumerated().find_map(|(new_id, known)| {
            if data.is_empty() {
                Some(DataRef::new(new_id, 0))
            } else {
                memmem::find(known, data).map(|offset| {
                    DataRef::new(new_id, u32::try_from(offset).expect("data offset exceeds `u32`"))
                })
            }
        });
        let data_ref = contained.unwrap_or_else(|| DataRef::new(packed.push(data.clone()), 0));
        remap.insert(old_id, data_ref);
    }

    let changed = packed != module.data;
    module.data = packed;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Data(data)) = &mut inst.value {
                let base = remap[&data.id];
                data.id = base.id;
                data.offset = data.offset.checked_add(base.offset).expect("data offset overflow");
            }
        }
    }
    changed
}
