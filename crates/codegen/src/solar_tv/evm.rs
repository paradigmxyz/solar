//! Complete-block EVM windows with fixed incoming stack heights and stable label identities.
//!
//! This adapter never selects instruction slices. It checks a changed block only when its
//! identity, shared declarations and entry height survive. Unknown heights and structural edits
//! remain unsupported. The checker models outgoing transfers, not the successor's execution.

use super::{Region, Snapshot};
use solar_evmir::ir::{BlockId, Module, verify::Verifier};
use solar_interface::Result;
use solar_sema::Gcx;
use std::collections::BTreeSet;

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
        if prefix(&self.snapshot.before) != prefix(&after)
            || self.before.blocks.iter().map(|b| b.label).ne(module.blocks.iter().map(|b| b.label))
        {
            return self.snapshot.check(
                gcx,
                "evmir",
                pass,
                module.name(),
                &after,
                (None, Some(
                    "shared EVM declarations or label set changed; block correspondence is unknown",
                )),
            );
        }
        let version = gcx.sess.opts.evm_version;
        let old_depths = Verifier::stack_entry_exact_depths(&self.before, version);
        let new_depths = Verifier::stack_entry_exact_depths(module, version);
        for (old_id, before) in self.before.blocks.iter_enumerated() {
            let id = old_id;
            if self.before.block_text(old_id).to_string() == module.block_text(id).to_string() {
                continue;
            }
            let height = old_depths[old_id];
            let rejection = if height.is_none() {
                Some("incoming EVM stack height is unknown")
            } else if new_depths[id] != height {
                Some("incoming EVM stack height changed")
            } else if before.metadata.call_effect != module.blocks[id].metadata.call_effect {
                Some("helper call interface changed")
            } else if boundaries(&self.before, old_id) != boundaries(module, id) {
                Some("window boundaries changed; successor equivalence is not modeled")
            } else {
                None
            };
            let region = Region { kind: "window", name: format!("bb{}", before.label), height };
            self.snapshot.check(
                gcx,
                "evmir",
                pass,
                module.name(),
                &after,
                (Some(&region), rejection),
            )?;
        }
        Ok(())
    }
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
