//! EVM windows, contextual prefixes and acyclic regions.
//!
//! This adapter never selects instruction slices. It checks a changed block only when its
//! identity, shared declarations and entry height survive. Adjacent edits, changed successors
//! and shared-tail changes use a complete reachable region through terminal outcomes instead.
//! Internal labels and stack layouts may change there. The checker rejects cycles, dynamic
//! transfers and operations outside its fragment. Unknown-height block windows can instead
//! prove an effect-free prefix for every incoming stack with an unchanged continuation.

use super::{Region, Snapshot};
use solar_evmir::ir::{BlockId, Module, verify::Verifier};
use solar_interface::Result;
use solar_sema::Gcx;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct EvmSnapshot<'a> {
    snapshot: Snapshot<'a>,
    before: Module,
}

impl<'a> EvmSnapshot<'a> {
    pub(crate) fn capture(gcx: Gcx<'a>, cached: bool, module: &Module) -> Option<Self> {
        let snapshot = Snapshot::capture(gcx, cached, module.to_text())?;
        Some(Self { snapshot, before: module.clone() })
    }

    pub(crate) fn check(self, gcx: Gcx<'_>, pass: &str, module: &Module) -> Result {
        let after = module.to_text().to_string();
        if self.snapshot.before == after {
            return Ok(());
        }
        let prefix = |text: &str| {
            text.lines().take_while(|line| !line.starts_with("bb")).collect::<Vec<_>>().join("\n")
        };
        if prefix(&self.snapshot.before) != prefix(&after) {
            return self.snapshot.check(
                gcx,
                "evmir",
                pass,
                module.name(),
                &after,
                (None, Some("shared EVM declarations changed; region correspondence is unknown")),
            );
        }
        let version = gcx.sess.opts.evm_version;
        let old_depths = Verifier::stack_entry_exact_depths(&self.before, version);
        let new_depths = Verifier::stack_entry_exact_depths(module, version);
        let labels = module
            .blocks
            .iter_enumerated()
            .map(|(id, block)| (block.label, id))
            .collect::<BTreeMap<_, _>>();
        let structural =
            self.before.blocks.iter().map(|b| b.label).ne(module.blocks.iter().map(|b| b.label));
        let mut checked = false;
        for (old_id, before) in self.before.blocks.iter_enumerated() {
            let Some(&id) = labels.get(&before.label) else { continue };
            let next = |m: &Module, block| m.next_block(block).map(|id| m.blocks[id].label);
            if self.before.block_text(old_id).to_string() == module.block_text(id).to_string()
                && next(&self.before, old_id) == next(module, id)
            {
                continue;
            }
            checked = true;
            let height = old_depths[old_id];
            let rejection = if height.is_none() {
                Some("incoming EVM stack height is unknown")
            } else if new_depths[id] != height {
                Some("incoming EVM stack height changed")
            } else if before.metadata.call_effect != module.blocks[id].metadata.call_effect {
                Some("helper call interface changed")
            } else {
                None
            };
            let use_region = structural
                || boundaries(&self.before, old_id) != boundaries(module, id)
                || neighbors_changed(&self.before, module, old_id, &labels);
            let region = Region {
                kind: if use_region { "region" } else { "window" },
                name: format!("bb{}", before.label),
                height,
            };
            self.snapshot.check(
                gcx,
                "evmir",
                pass,
                module.name(),
                &after,
                (Some(&region), rejection),
            )?;
        }
        if structural && !checked {
            return self.snapshot.check(
                gcx,
                "evmir",
                pass,
                module.name(),
                &after,
                (None, Some("no surviving changed entry for EVM region correspondence")),
            );
        }
        Ok(())
    }
}

fn neighbors_changed(
    before: &Module,
    after: &Module,
    entry: BlockId,
    labels: &BTreeMap<u32, BlockId>,
) -> bool {
    let name = before.blocks[entry].label;
    let mut neighbors = boundaries(before, entry);
    if let Some(&id) = labels.get(&name) {
        neighbors.extend(boundaries(after, id));
    }
    for (id, block) in before.blocks.iter_enumerated() {
        if boundaries(before, id).contains(&name) {
            neighbors.insert(block.label);
        }
    }
    for (id, block) in after.blocks.iter_enumerated() {
        if boundaries(after, id).contains(&name) {
            neighbors.insert(block.label);
        }
    }
    before.blocks.iter_enumerated().any(|(id, block)| {
        neighbors.contains(&block.label)
            && labels.get(&block.label).is_none_or(|&new| {
                before.block_text(id).to_string() != after.block_text(new).to_string()
            })
    })
}

fn boundaries(module: &Module, id: BlockId) -> BTreeSet<u32> {
    let block = &module.blocks[id];
    let mut targets = block
        .instructions
        .iter()
        .filter_map(|inst| inst.pushed_block())
        .map(|target| module.blocks[target].label)
        .collect::<BTreeSet<_>>();
    if let Some(term) = &block.terminator {
        term.kind.visit_targets(|target| {
            targets.insert(module.blocks[target].label);
        });
    }
    targets
}
