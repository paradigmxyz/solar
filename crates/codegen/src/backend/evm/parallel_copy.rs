//! Scheduling of simultaneous copies between private spill homes.
//!
//! A destination can be written as soon as no pending copy reads its old value.
//! Read counts select these sinks in quadratic time without a dependency graph.
//! A remaining cycle is broken by saving one old destination in the reserved
//! scratch home and redirecting its pending reads. Disjoint cycles reuse that
//! home. Identity transfers disappear. Rematerialized values have no memory-home
//! dependency. Duplicate destinations conservatively retain the original complete
//! staging protocol, preserving its last-write order.
//!
//! This private MIR-to-EVM boundary helper plans copies only; the machine emitter
//! resolves frame addresses and emits loads/stores with their usual stack peaks.

use crate::mir;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// A homed word or a MIR value supplied by the stack or rematerialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Home(usize),
    Value(mir::ValueId),
}

/// Orders copies while preserving every source from the incoming memory state.
///
/// `scratch` and the following staging words must be disjoint from all homes.
pub(crate) fn schedule(mut pending: Vec<(Source, usize)>, scratch: usize) -> Vec<(Source, usize)> {
    let mut destinations = FxHashSet::default();
    if pending.iter().any(|&(_, home)| !destinations.insert(home)) {
        return stage(&pending, scratch);
    }
    pending.retain(|&(source, home)| source != Source::Home(home));
    let mut reads = FxHashMap::<usize, usize>::default();
    for &(source, _) in &pending {
        if let Source::Home(home) = source {
            *reads.entry(home).or_default() += 1;
        }
    }
    let mut result = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        if let Some(index) = pending.iter().position(|(_, home)| !reads.contains_key(home)) {
            let (source, home) = pending.remove(index);
            if let Source::Home(source_home) = source {
                let count = reads.get_mut(&source_home).unwrap();
                *count -= 1;
                if *count == 0 {
                    reads.remove(&source_home);
                }
            }
            // destination = incoming_source
            result.push((source, home));
        } else {
            let home = pending[0].1;
            // scratch = old_destination
            // remaining reads of old_destination -> scratch
            result.push((Source::Home(home), scratch));
            let count = reads.remove(&home).unwrap();
            reads.insert(scratch, count);
            for (source, _) in &mut pending {
                if *source == Source::Home(home) {
                    *source = Source::Home(scratch);
                }
            }
        }
    }
    result
}

/// Stages all incoming words before publishing destinations in their original order.
pub(crate) fn stage(pending: &[(Source, usize)], scratch: usize) -> Vec<(Source, usize)> {
    // scratch[i] = incoming[i]
    // destination[i] = scratch[i]
    pending
        .iter()
        .enumerate()
        .map(|(i, &(source, _))| (source, scratch + i))
        .chain(pending.iter().enumerate().map(|(i, &(_, home))| (Source::Home(scratch + i), home)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_sources_cycles_aliases_and_literals() {
        // Exhaust every four-destination source map, including fan-out, identity,
        // cycles, disjoint cycles and a rematerialized value. Repeat with aliased
        // destinations to exercise the conservative staging path.
        for encoded in 0usize..5usize.pow(4) {
            for destinations in [[0, 1, 2, 3], [0, 1, 0, 3]] {
                let mut remaining = encoded;
                let copies = destinations
                    .into_iter()
                    .map(|home| {
                        let source = remaining % 5;
                        remaining /= 5;
                        (
                            if source == 4 {
                                Source::Value(mir::ValueId::from_usize(0))
                            } else {
                                Source::Home(source)
                            },
                            home,
                        )
                    })
                    .collect::<Vec<_>>();
                let initial = [11, 22, 33, 44, 0, 0, 0, 0];
                let mut expected = initial;
                for &(source, home) in &copies {
                    expected[home] = match source {
                        Source::Home(home) => initial[home],
                        Source::Value(_) => 55,
                    };
                }
                let ordered = schedule(copies, 4);
                let mut actual = initial;
                for (source, home) in ordered {
                    actual[home] = match source {
                        Source::Home(home) => actual[home],
                        Source::Value(_) => 55,
                    };
                }
                assert_eq!(
                    actual[..4],
                    expected[..4],
                    "source map {encoded}, destinations {destinations:?}"
                );
            }
        }
    }
}
