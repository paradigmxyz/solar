//! Layout shuffling: rearranging a whole modeled stack into a target layout.
//!
//! A target names a specific word ([`Want::Value`] or [`Want::Ret`]) or accepts anything
//! ([`Want::Any`]) at each position. The shuffle runs in two phases:
//!
//! 1. Counts: pop or swap-and-pop surplus words that no open position can absorb, copy each missing
//!    word to the top (`DUP`, or a fresh push for materializable values), and push fillers until
//!    the height matches.
//! 2. Order: with the multiset now right, follow permutation cycles through the top. The top word
//!    goes to the deepest misplaced position that accepts it; when the top is already in place, the
//!    deepest misplaced word is brought up to start the next cycle. Surplus words belong in open
//!    positions, and needed words only in positions that name them.
//!
//! Each `SWAP` in the second phase settles one word or opens one cycle, so the phase is
//! optimal for permutations that only exchange through the top. The first phase favors
//! cheap removals at the top and does not search, so a combined count and order change can
//! cost a few more operations than the shortest sequence.
//!
//! When a word that must move lies beyond reach, [`rebuild`] offers a fallback that touches
//! only the top: it pops down to the deepest words already in place and pushes the rest of the
//! target from fresh copies and copies of the kept words.

use super::{Slot, Want};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// One shuffle operation. Depths are counted from the top, which has depth zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Move {
    /// Exchanges the top with the word at this depth.
    Swap(usize),
    /// Copies the word at this depth to the top.
    Dup(usize),
    /// Pushes a fresh copy of a materializable value or a spilled return address.
    Fresh(Slot),
    /// Pushes a word that nothing reads.
    Filler,
    /// Removes the top word.
    Pop,
}

/// Why a shuffle failed: the word at this position from the bottom is beyond reach.
#[derive(Clone, Copy, Debug)]
pub(super) struct Unreachable(pub(super) Option<usize>);

/// Computes moves that turn `stack` into `target`. `fresh` reports words that can be pushed
/// without a stack copy. Every `SWAP` and `DUP` stays within `reach`.
pub(super) fn shuffle(
    stack: &[Slot],
    target: &[Want],
    reach: usize,
    fresh: &dyn Fn(Slot) -> bool,
) -> Result<Vec<Move>, Unreachable> {
    let mut state = State { stack: stack.to_vec(), moves: Vec::new(), reach };
    let n = target.len();

    let mut need: FxHashMap<Slot, usize> = FxHashMap::default();
    let mut open = 0usize;
    for want in target {
        match *want {
            Want::Value(value) => *need.entry(Slot::Value(value)).or_default() += 1,
            Want::Ret => *need.entry(Slot::Ret).or_default() += 1,
            Want::Any => open += 1,
        }
    }
    let needed = |slot: Slot| need.get(&slot).copied().unwrap_or(0);

    // Phase 1: counts.
    let surplus_count = |stack: &[Slot]| -> usize {
        let mut seen: FxHashMap<Slot, usize> = FxHashMap::default();
        stack
            .iter()
            .filter(|&&slot| {
                if slot == Slot::Junk {
                    return true;
                }
                let count = seen.entry(slot).or_default();
                *count += 1;
                *count > needed(slot)
            })
            .count()
    };
    // Whether the word at `index` is one of its value's surplus copies; the shallowest copies
    // count as surplus, so deeper copies stay in place.
    let is_surplus = |stack: &[Slot], index: usize| -> bool {
        let slot = stack[index];
        if slot == Slot::Junk {
            return true;
        }
        let deeper = stack[..index].iter().filter(|&&s| s == slot).count();
        deeper >= needed(slot)
    };
    loop {
        let surplus = surplus_count(&state.stack);
        if surplus <= open {
            break;
        }
        // Drop the shallowest surplus word, preferring the top.
        let h = state.stack.len();
        let Some(index) = (0..h).rev().find(|&i| is_surplus(&state.stack, i)) else { break };
        if index + 1 != h {
            state.swap(h - 1 - index)?;
        }
        state.pop();
    }
    // Create missing words.
    let mut missing: Vec<Slot> = Vec::new();
    for (&slot, &count) in &need {
        let have = state.stack.iter().filter(|&&s| s == slot).count();
        for _ in have..count {
            missing.push(slot);
        }
    }
    // Create in target order so fresh copies tend to land near their positions.
    missing.sort_by_key(|slot| {
        target.iter().position(|want| want.accepts(*slot)).unwrap_or(usize::MAX)
    });
    for slot in missing {
        match slot {
            slot if fresh(slot) => state.push(Move::Fresh(slot), slot),
            _ => {
                let Some(depth) = state.depth_of(slot) else {
                    return Err(Unreachable(None));
                };
                state.dup(depth)?;
            }
        }
    }
    while state.stack.len() < n {
        state.push(Move::Filler, Slot::Junk);
    }
    debug_assert!(state.stack.len() == n, "shuffle counts do not match the target height");

    // Phase 2: order. Assign every word a destination: a position that names its value, or any
    // open position. Words already at a position naming them keep it.
    const OPEN: usize = usize::MAX;
    let mut dest = vec![OPEN; n];
    let mut taken = DenseBitSet::<usize>::new_empty(n);
    for i in 0..n {
        if target[i] != Want::Any && target[i].accepts(state.stack[i]) {
            dest[i] = i;
            taken.insert(i);
        }
    }
    for (j, want) in target.iter().enumerate() {
        if *want == Want::Any || taken.contains(j) {
            continue;
        }
        // Take the shallowest unassigned word holding the named value.
        let word = (0..n).rev().find(|&i| dest[i] == OPEN && want.accepts(state.stack[i]));
        let Some(word) = word else { return Err(Unreachable(None)) };
        dest[word] = j;
    }
    // `dest` is indexed by current position and moves with its word.
    let settled = |dest: &[usize], i: usize| {
        if dest[i] == OPEN { target[i] == Want::Any } else { dest[i] == i }
    };
    let limit = 2 * n + 2;
    for _ in 0..limit {
        let h = n;
        if (0..h).all(|i| settled(&dest, i)) {
            return Ok(state.moves);
        }
        let top = h - 1;
        let home = if settled(&dest, top) {
            None
        } else if dest[top] == OPEN {
            // An open position whose word belongs elsewhere.
            (0..top).find(|&i| target[i] == Want::Any && !settled(&dest, i))
        } else {
            Some(dest[top])
        };
        if let Some(home) = home {
            if state.stack[home] == state.stack[top] {
                // Equal words: exchange their destinations instead of the words.
                dest.swap(home, top);
                continue;
            }
            state.swap(top - home)?;
            dest.swap(home, top);
            continue;
        }
        // Open the next cycle with the deepest misplaced word that a named position wants.
        let Some(deepest) = (0..top)
            .find(|&i| !settled(&dest, i) && dest[i] != OPEN)
            .or_else(|| (0..top).find(|&i| !settled(&dest, i)))
        else {
            break;
        };
        state.swap(top - deepest)?;
        dest.swap(deepest, top);
    }
    Err(Unreachable(None))
}

/// Turns `stack` into `target` by popping down to the deepest words that already satisfy it
/// and pushing every word above them: fresh words anew, others as copies of kept words within
/// `reach`. Fails with a word that has neither a fresh push nor a reachable copy.
pub(super) fn rebuild(
    stack: &[Slot],
    target: &[Want],
    reach: usize,
    fresh: &dyn Fn(Slot) -> bool,
) -> Result<Vec<Move>, Slot> {
    let kept = stack.iter().zip(target).take_while(|&(&slot, want)| want.accepts(slot)).count();
    let mut state = State { stack: stack.to_vec(), moves: Vec::new(), reach };
    while state.stack.len() > kept {
        state.pop();
    }
    for &want in &target[kept..] {
        let slot = match want {
            Want::Any => {
                state.push(Move::Filler, Slot::Junk);
                continue;
            }
            Want::Ret => Slot::Ret,
            Want::Value(value) => Slot::Value(value),
        };
        if fresh(slot) {
            state.push(Move::Fresh(slot), slot);
        } else if let Some(depth) = state.depth_of(slot)
            && depth < reach
        {
            state.push(Move::Dup(depth), slot);
        } else {
            return Err(slot);
        }
    }
    Ok(state.moves)
}

struct State {
    stack: Vec<Slot>,
    moves: Vec<Move>,
    reach: usize,
}

impl State {
    fn depth_of(&self, slot: Slot) -> Option<usize> {
        self.stack.iter().rev().position(|&s| s == slot)
    }

    fn swap(&mut self, depth: usize) -> Result<(), Unreachable> {
        let h = self.stack.len();
        if depth == 0 || depth > self.reach || depth >= h {
            return Err(Unreachable(h.checked_sub(depth + 1)));
        }
        self.stack.swap(h - 1, h - 1 - depth);
        self.moves.push(Move::Swap(depth));
        Ok(())
    }

    fn dup(&mut self, depth: usize) -> Result<(), Unreachable> {
        let h = self.stack.len();
        if depth >= self.reach || depth >= h {
            return Err(Unreachable(h.checked_sub(depth + 1)));
        }
        self.stack.push(self.stack[h - 1 - depth]);
        self.moves.push(Move::Dup(depth));
        Ok(())
    }

    fn push(&mut self, mv: Move, slot: Slot) {
        self.stack.push(slot);
        self.moves.push(mv);
    }

    fn pop(&mut self) {
        self.stack.pop();
        self.moves.push(Move::Pop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::ValueId;
    use std::collections::{HashMap, VecDeque};

    fn value(i: usize) -> Slot {
        Slot::Value(ValueId::from_usize(i))
    }

    fn apply(stack: &mut Vec<Slot>, mv: Move) {
        let h = stack.len();
        match mv {
            Move::Swap(d) => stack.swap(h - 1, h - 1 - d),
            Move::Dup(d) => stack.push(stack[h - 1 - d]),
            Move::Fresh(slot) => stack.push(slot),
            Move::Filler => stack.push(Slot::Junk),
            Move::Pop => {
                stack.pop();
            }
        }
    }

    fn matches(stack: &[Slot], target: &[Want]) -> bool {
        stack.len() == target.len() && target.iter().zip(stack).all(|(w, &s)| w.accepts(s))
    }

    /// Breadth-first shortest move count, for small layouts.
    fn shortest(stack: &[Slot], target: &[Want], fresh: &dyn Fn(Slot) -> bool) -> Option<usize> {
        let mut seen = HashMap::new();
        let mut queue = VecDeque::new();
        seen.insert(stack.to_vec(), 0usize);
        queue.push_back(stack.to_vec());
        let values: Vec<Slot> = target
            .iter()
            .filter_map(|w| match *w {
                Want::Value(v) => Some(Slot::Value(v)),
                _ => None,
            })
            .collect();
        while let Some(state) = queue.pop_front() {
            let dist = seen[&state];
            if matches(&state, target) {
                return Some(dist);
            }
            if dist >= 9 || state.len() > target.len() + 3 {
                continue;
            }
            let h = state.len();
            let mut next = Vec::new();
            for d in 1..h {
                next.push(Move::Swap(d));
            }
            for d in 0..h {
                next.push(Move::Dup(d));
            }
            for &slot in &values {
                if fresh(slot) {
                    next.push(Move::Fresh(slot));
                }
            }
            next.push(Move::Filler);
            if h > 0 {
                next.push(Move::Pop);
            }
            for mv in next {
                let mut s = state.clone();
                apply(&mut s, mv);
                if !seen.contains_key(&s) {
                    seen.insert(s.clone(), dist + 1);
                    queue.push_back(s);
                }
            }
        }
        None
    }

    /// Exhaustively checks small layouts: the shuffle always reaches the target, and its move
    /// count stays close to the shortest sequence.
    #[test]
    fn reaches_small_targets() {
        let slots = [value(0), value(1), value(2), Slot::Ret, Slot::Junk];
        let wants = [
            Want::Value(ValueId::from_usize(0)),
            Want::Value(ValueId::from_usize(1)),
            Want::Value(ValueId::from_usize(2)),
            Want::Ret,
            Want::Any,
        ];
        let fresh = |slot: Slot| slot == value(2);
        let mut extra = 0usize;
        let mut cases = 0usize;
        let mut sources = vec![vec![]];
        for _ in 0..4 {
            let mut longer = Vec::new();
            for s in &sources {
                for &slot in &slots {
                    let mut s = s.clone();
                    s.push(slot);
                    longer.push(s);
                }
            }
            sources.extend(longer);
        }
        let mut unique = std::collections::HashSet::new();
        sources.retain(|source| unique.insert(source.clone()));
        let mut targets = vec![vec![]];
        for _ in 0..3 {
            let mut longer = Vec::new();
            for t in &targets {
                for &want in &wants {
                    let mut t = t.clone();
                    t.push(want);
                    longer.push(t);
                }
            }
            targets.extend(longer);
        }
        for source in &sources {
            if source.iter().filter(|&&s| s == Slot::Ret).count() > 1 {
                continue;
            }
            for target in &targets {
                if target.iter().filter(|&&w| w == Want::Ret).count()
                    != source.iter().filter(|&&s| s == Slot::Ret).count()
                {
                    continue;
                }
                // Every non-fresh wanted value must exist in the source.
                let available = target.iter().all(|w| match *w {
                    Want::Value(v) => fresh(Slot::Value(v)) || source.contains(&Slot::Value(v)),
                    _ => true,
                });
                if !available {
                    continue;
                }
                let moves = shuffle(source, target, 16, &fresh)
                    .unwrap_or_else(|_| panic!("stuck: {source:?} -> {target:?}"));
                let mut stack = source.clone();
                for &mv in &moves {
                    apply(&mut stack, mv);
                }
                assert!(matches(&stack, target), "{source:?} -> {target:?}: {moves:?}");
                if source.len() <= 2 {
                    let best = shortest(source, target, &fresh).unwrap();
                    assert!(moves.len() >= best);
                    extra += moves.len() - best;
                    cases += 1;
                }
            }
        }
        // Average excess over the shortest sequence, in moves per case.
        assert!(extra * 10 < cases * 3, "excess {extra} over {cases} cases");
    }
}
