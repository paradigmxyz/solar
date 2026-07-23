//! Width-aware helpers for backend-owned memory layout decisions.

/// One emitted address whose value changes under a proposed layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RelayoutAddress {
    /// Address before the proposed layout change.
    pub before: u64,
    /// Address after the proposed layout change.
    pub after: u64,
    /// Number of emitted pushes that reference this address.
    pub references: usize,
}

/// Returns whether a proposed relayout widens none of its emitted address pushes.
///
/// Reference counts are part of the API because callers already collect them
/// for layout costing, even though a single widening is enough to reject a
/// width-preserving proposal.
pub(super) fn preserves_push_width(addresses: impl IntoIterator<Item = RelayoutAddress>) -> bool {
    addresses.into_iter().all(|address| {
        address.references == 0 || push_width(address.after) <= push_width(address.before)
    })
}

/// Minimum number of immediate bytes needed to push a non-negative address.
const fn push_width(value: u64) -> u8 {
    if value == 0 { 0 } else { ((u64::BITS - value.leading_zeros()).div_ceil(8)) as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_growth_within_a_push_width() {
        assert!(preserves_push_width([RelayoutAddress {
            before: 0xc0,
            after: 0xe0,
            references: 4,
        }]));
    }

    #[test]
    fn rejects_growth_across_a_push_width() {
        assert!(!preserves_push_width([RelayoutAddress {
            before: 0xff,
            after: 0x100,
            references: 1,
        }]));
        assert!(!preserves_push_width([RelayoutAddress {
            before: 0xffff,
            after: 0x1_0000,
            references: 1,
        }]));
    }

    #[test]
    fn ignores_unreferenced_addresses() {
        assert!(preserves_push_width([RelayoutAddress {
            before: 0xff,
            after: 0x100,
            references: 0,
        }]));
    }
}
