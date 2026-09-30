//! Path-sensitive contents of exact storage slots, relative to function entry.
//!
//! Guards such as `require(!locked); locked = true;`, OpenZeppelin's `_status` checks, or an
//! `onlyOwner` modifier compare values loaded from fixed slots and branch on the result.
//! This domain tracks those slots symbolically so that summaries can state them in terms of
//! the caller's state:
//!
//! - A [`SymWord`] describes a word bit by bit: some bits are known constants, and some are copies
//!   of a slot's value at function entry, possibly shifted. This covers packed booleans and
//!   addresses read through `and`/`shr` masks and written with read-modify-write `or`s.
//! - A [`Pred`] is a branch condition over entry bits, `caller`, or a constant.
//! - [`SlotState`] maps each written exact slot to its current word, records constraints on entry
//!   values learned from branches and checks, and which owner-like origins `caller` was compared
//!   equal to. Edges whose condition contradicts the known state are infeasible, which is how a set
//!   lock refutes reentrant paths through a guarded function.
//!
//! Only absolute slots are tracked; hashed locations never alias them under the storage
//! layout assumptions of [`storage_path`](super::storage_path). Writes through unknown slots
//! or storage-pointer parameters clobber the tracked state conservatively.

use super::lattice::JoinSemiLattice;
use crate::mir::{FunctionId, ImmutableId, InstId, ValueId};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// Maximum excluded values remembered for one entry field.
const MAX_EXCLUDED: usize = 8;

/// An absolute persistent or transient storage slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct SlotKey {
    /// Whether the slot is in transient storage.
    pub(crate) transient: bool,
    /// The slot number.
    pub(crate) slot: U256,
}

impl fmt::Display for SlotKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let space = if self.transient { "tslot" } else { "slot" };
        write!(f, "{space}({})", DisplayWord(self.slot))
    }
}

/// Bits of a word that copy bits of a slot's entry value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EntryBits {
    /// The slot whose entry value is copied.
    pub(crate) slot: SlotKey,
    /// Word bits that are copies.
    pub(crate) mask: U256,
    /// Word bit `i` copies entry bit `i + shift`.
    pub(crate) shift: i32,
}

/// A word with known constant bits and bits copied from one slot's entry value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SymWord {
    /// Bits with a known value.
    pub(crate) known: U256,
    /// The known bits' values; zero elsewhere.
    pub(crate) value: U256,
    /// Bits that copy an entry value; disjoint from `known`.
    pub(crate) entry: Option<EntryBits>,
}

fn shift_bits(bits: U256, shift: i32) -> U256 {
    if shift >= 0 { bits << shift as usize } else { bits >> shift.unsigned_abs() as usize }
}

impl SymWord {
    /// A word about which nothing is known.
    pub(crate) const UNKNOWN: Self = Self { known: U256::ZERO, value: U256::ZERO, entry: None };

    /// A known constant.
    pub(crate) const fn constant(value: U256) -> Self {
        Self { known: U256::MAX, value, entry: None }
    }

    /// The entry value of `slot`.
    pub(crate) const fn entry(slot: SlotKey) -> Self {
        Self {
            known: U256::ZERO,
            value: U256::ZERO,
            entry: Some(EntryBits { slot, mask: U256::MAX, shift: 0 }),
        }
    }

    /// Returns the constant value, if every bit is known.
    pub(crate) fn as_constant(&self) -> Option<U256> {
        (self.known == U256::MAX).then_some(self.value)
    }

    /// Returns whether the bits in `mask` still equal `slot`'s entry value, unshifted.
    pub(crate) fn is_entry_identity(&self, slot: SlotKey, mask: U256) -> bool {
        self.entry.is_some_and(|entry| {
            entry.slot == slot && entry.shift == 0 && entry.mask & mask == mask
        })
    }

    /// Returns whether every bit is either known or copied from an entry value.
    fn is_determined(&self) -> bool {
        self.known | self.entry.map_or(U256::ZERO, |entry| entry.mask) == U256::MAX
    }

    fn normalize(mut self) -> Self {
        self.value &= self.known;
        if let Some(entry) = &mut self.entry {
            entry.mask &= !self.known;
            if entry.mask.is_zero() {
                self.entry = None;
            }
        }
        self
    }

    /// Bitwise and.
    pub(crate) fn and(self, other: Self) -> Self {
        let zeros = (self.known & !self.value) | (other.known & !other.value);
        let ones = self.known & self.value & other.known & other.value;
        let known = zeros | ones;
        // A copied bit survives where the other operand is a known one.
        let entry = match (self.entry, other.entry) {
            (Some(entry), _) if other.known & other.value & entry.mask != U256::ZERO => {
                Some(EntryBits { mask: entry.mask & other.known & other.value, ..entry })
            }
            (_, Some(entry)) if self.known & self.value & entry.mask != U256::ZERO => {
                Some(EntryBits { mask: entry.mask & self.known & self.value, ..entry })
            }
            _ => None,
        };
        Self { known, value: ones, entry }.normalize()
    }

    /// Bitwise or.
    pub(crate) fn or(self, other: Self) -> Self {
        let ones = (self.known & self.value) | (other.known & other.value);
        let zeros = self.known & !self.value & other.known & !other.value;
        let known = zeros | ones;
        // A copied bit survives where the other operand is a known zero.
        let copied = |entry: Option<EntryBits>, other: Self| {
            entry.map(|entry| EntryBits { mask: entry.mask & other.known & !other.value, ..entry })
        };
        let entry = match (copied(self.entry, other), copied(other.entry, self)) {
            (Some(a), Some(b)) if a.slot == b.slot && a.shift == b.shift => {
                Some(EntryBits { mask: a.mask | b.mask, ..a })
            }
            (Some(a), Some(b)) => {
                Some(if a.mask.count_ones() >= b.mask.count_ones() { a } else { b })
            }
            (a, b) => a.or(b),
        };
        Self { known, value: ones, entry }.normalize()
    }

    /// Bitwise xor; only known bits survive.
    pub(crate) fn xor(self, other: Self) -> Self {
        let known = self.known & other.known;
        Self { known, value: (self.value ^ other.value) & known, entry: None }
    }

    /// Bitwise not; copied bits become unknown.
    pub(crate) fn not(self) -> Self {
        Self { known: self.known, value: !self.value & self.known, entry: None }
    }

    /// Logical right shift by a constant.
    pub(crate) fn shr(self, amount: usize) -> Self {
        if amount >= 256 {
            return Self::constant(U256::ZERO);
        }
        let high = !(U256::MAX >> amount);
        Self {
            known: (self.known >> amount) | high,
            value: self.value >> amount,
            entry: self.entry.map(|entry| EntryBits {
                mask: entry.mask >> amount,
                shift: entry.shift + amount as i32,
                ..entry
            }),
        }
        .normalize()
    }

    /// Left shift by a constant.
    pub(crate) fn shl(self, amount: usize) -> Self {
        if amount >= 256 {
            return Self::constant(U256::ZERO);
        }
        let low = !(U256::MAX << amount);
        Self {
            known: (self.known << amount) | low,
            value: self.value << amount,
            entry: self.entry.map(|entry| EntryBits {
                mask: entry.mask << amount,
                shift: entry.shift - amount as i32,
                ..entry
            }),
        }
        .normalize()
    }

    /// Keeps the low `bits` bits.
    pub(crate) fn trunc(self, bits: u32) -> Self {
        if bits >= 256 {
            return self;
        }
        self.and(Self::constant((U256::from(1) << bits as usize) - U256::from(1)))
    }

    /// Substitutes the entry values this word copies with `current` values.
    pub(crate) fn compose(self, current: impl Fn(SlotKey) -> Self) -> Self {
        let Some(entry) = self.entry else { return self };
        let source = current(entry.slot);
        let shifted = if entry.shift >= 0 {
            source.shr(entry.shift as usize)
        } else {
            source.shl(entry.shift.unsigned_abs() as usize)
        };
        let copied = shifted.and(Self::constant(entry.mask));
        let base = Self { known: self.known, value: self.value, entry: None };
        // The copied bits lie outside `known`, so the or merges disjoint parts.
        Self {
            known: base.known | (copied.known & entry.mask),
            value: base.value | (copied.value & entry.mask),
            entry: copied.entry,
        }
        .normalize()
    }
}

impl JoinSemiLattice for SymWord {
    fn join(&mut self, other: &Self) -> bool {
        let known = self.known & other.known & !(self.value ^ other.value);
        let entry = match (self.entry, other.entry) {
            (Some(a), Some(b)) if a.slot == b.slot && a.shift == b.shift => {
                let mask = a.mask & b.mask;
                (!mask.is_zero()).then_some(EntryBits { mask, ..a })
            }
            _ => None,
        };
        let joined = Self { known, value: self.value & known, entry }.normalize();
        let changed = joined != *self;
        *self = joined;
        changed
    }
}

impl fmt::Display for SymWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = self.as_constant() {
            return DisplayWord(value).fmt(f);
        }
        match self.entry {
            Some(entry) if entry.mask == U256::MAX && entry.shift == 0 => {
                write!(f, "entry({})", entry.slot)
            }
            Some(entry) => {
                write!(f, "(entry({}) & {:#x}", entry.slot, shift_bits(entry.mask, entry.shift))?;
                if entry.shift != 0 {
                    write!(f, " >> {}", entry.shift)?;
                }
                if !self.known.is_zero() && !self.value.is_zero() {
                    write!(f, " | {:#x}", self.value)?;
                }
                write!(f, ")")
            }
            None if self.known.is_zero() => write!(f, "?"),
            None => write!(f, "(? & {:#x} | {:#x})", !self.known, self.value),
        }
    }
}

/// A value `caller` may be compared against to establish ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Origin {
    /// Bits of a slot's entry value.
    Slot {
        /// The slot.
        slot: SlotKey,
        /// The compared bits, in slot coordinates.
        mask: U256,
    },
    /// An immutable.
    Immutable(ImmutableId),
    /// A hard-coded address.
    Constant(U256),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Slot { slot, mask } if *mask == U256::MAX => write!(f, "entry({slot})"),
            Self::Slot { slot, mask } => write!(f, "entry({slot}) & {mask:#x}"),
            Self::Immutable(id) => write!(f, "immutable{}", id.index()),
            Self::Constant(value) => write!(f, "{value:#x}"),
        }
    }
}

/// A condition over entry values or `caller`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Pred {
    /// `entry(slot) & mask == value` (or `!=`), in slot coordinates.
    Field {
        /// The slot.
        slot: SlotKey,
        /// Compared bits.
        mask: U256,
        /// Expected bits.
        value: U256,
        /// Whether the condition is an equality.
        eq: bool,
    },
    /// `caller == origin` (or `!=`).
    Caller {
        /// The compared origin.
        origin: Origin,
        /// Whether the condition is an equality.
        eq: bool,
    },
    /// A constant condition.
    Const(bool),
}

impl Pred {
    /// Returns the negated condition.
    pub(crate) const fn negate(self) -> Self {
        match self {
            Self::Field { slot, mask, value, eq } => Self::Field { slot, mask, value, eq: !eq },
            Self::Caller { origin, eq } => Self::Caller { origin, eq: !eq },
            Self::Const(value) => Self::Const(!value),
        }
    }

    /// Restates a callee condition over its entry in terms of the caller's `state`.
    ///
    /// Returns `None` when the condition cannot be expressed.
    pub(crate) fn compose(self, state: &SlotState) -> Option<Self> {
        match self {
            Self::Field { slot, mask, value, eq } => {
                let current = state.current(slot);
                if current.known & mask == mask {
                    return Some(Self::Const((current.value & mask == value) == eq));
                }
                current.is_entry_identity(slot, mask).then_some(self)
            }
            Self::Caller { origin: Origin::Slot { slot, mask }, eq } => {
                let current = state.current(slot);
                if current.known & mask == mask {
                    return Some(Self::Caller {
                        origin: Origin::Constant(current.value & mask),
                        eq,
                    });
                }
                current.is_entry_identity(slot, mask).then_some(self)
            }
            Self::Caller { .. } | Self::Const(_) => Some(self),
        }
    }
}

impl fmt::Display for Pred {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field { slot, mask, value, eq } => {
                let op = if *eq { "==" } else { "!=" };
                if *mask == U256::MAX {
                    write!(f, "entry({slot}) {op} {}", DisplayWord(*value))
                } else {
                    write!(f, "entry({slot}) & {mask:#x} {op} {}", DisplayWord(*value))
                }
            }
            Self::Caller { origin, eq } => {
                write!(f, "caller {} {origin}", if *eq { "==" } else { "!=" })
            }
            Self::Const(value) => write!(f, "{value}"),
        }
    }
}

/// What an SSA value is known to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Val {
    /// A symbolic word.
    Word(SymWord),
    /// A boolean condition.
    Pred(Pred),
    /// `msg.sender`, possibly masked or extended.
    Caller,
    /// An immutable's value.
    Immutable(ImmutableId),
}

impl Val {
    /// Returns the word view of this value, when it has one.
    pub(crate) fn word(self) -> Option<SymWord> {
        match self {
            Self::Word(word) => Some(word),
            Self::Pred(Pred::Const(value)) => Some(SymWord::constant(U256::from(u8::from(value)))),
            Self::Pred(_) | Self::Caller | Self::Immutable(_) => None,
        }
    }

    fn join(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Word(mut a), Self::Word(b)) => {
                a.join(&b);
                Some(Self::Word(a))
            }
            (a, b) if a == b => Some(a),
            _ => None,
        }
    }
}

/// Returns the condition `a == b`, when it can be expressed.
pub(crate) fn equality(a: Option<Val>, b: Option<Val>) -> Option<Pred> {
    let (a, b) = (a?, b?);
    match (a, b) {
        (Val::Word(x), Val::Word(y)) => word_equality(x, y),
        (Val::Pred(p), Val::Word(w)) | (Val::Word(w), Val::Pred(p)) => match w.as_constant()? {
            value if value.is_zero() => Some(p.negate()),
            value if value == U256::from(1) => Some(p),
            _ => Some(Pred::Const(false)),
        },
        (Val::Caller, Val::Immutable(id)) | (Val::Immutable(id), Val::Caller) => {
            Some(Pred::Caller { origin: Origin::Immutable(id), eq: true })
        }
        (Val::Caller, Val::Word(w)) | (Val::Word(w), Val::Caller) => {
            if let Some(value) = w.as_constant() {
                return Some(Pred::Caller { origin: Origin::Constant(value), eq: true });
            }
            let entry = w.entry?;
            let address = (U256::from(1) << 160) - U256::from(1);
            (w.is_determined() && w.known & !w.value == w.known && entry.mask == address).then(
                || Pred::Caller {
                    origin: Origin::Slot {
                        slot: entry.slot,
                        mask: shift_bits(entry.mask, entry.shift),
                    },
                    eq: true,
                },
            )
        }
        _ => None,
    }
}

fn word_equality(x: SymWord, y: SymWord) -> Option<Pred> {
    if let (Some(x), Some(y)) = (x.as_constant(), y.as_constant()) {
        return Some(Pred::Const(x == y));
    }
    let (word, constant) = match (x.as_constant(), y.as_constant()) {
        (None, Some(constant)) => (x, constant),
        (Some(constant), None) => (y, constant),
        _ => return None,
    };
    if !word.is_determined() {
        return None;
    }
    if (word.value ^ constant) & word.known != U256::ZERO {
        return Some(Pred::Const(false));
    }
    let entry = word.entry?;
    Some(Pred::Field {
        slot: entry.slot,
        mask: shift_bits(entry.mask, entry.shift),
        value: shift_bits(constant & entry.mask, entry.shift),
        eq: true,
    })
}

/// A must fact about an entry field.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Constraint {
    /// The field equals this value.
    Eq(U256),
    /// The field differs from every value in the set.
    Ne(BTreeSet<U256>),
}

impl Constraint {
    /// Returns whether `value` satisfies the constraint.
    pub(crate) fn admits(&self, value: U256) -> bool {
        match self {
            Self::Eq(expected) => *expected == value,
            Self::Ne(excluded) => !excluded.contains(&value),
        }
    }

    /// Returns whether both constraints can hold together.
    pub(crate) fn compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Eq(a), Self::Eq(b)) => a == b,
            (Self::Eq(a), Self::Ne(set)) | (Self::Ne(set), Self::Eq(a)) => !set.contains(a),
            (Self::Ne(_), Self::Ne(_)) => true,
        }
    }

    /// Returns the conjunction, or `None` if it is unsatisfiable.
    fn meet(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::Eq(a), Self::Eq(b)) => (a == b).then(|| self.clone()),
            (Self::Eq(a), Self::Ne(set)) | (Self::Ne(set), Self::Eq(a)) => {
                (!set.contains(a)).then_some(Self::Eq(*a))
            }
            (Self::Ne(a), Self::Ne(b)) => {
                let mut union = a.clone();
                union.extend(b.iter().copied());
                while union.len() > MAX_EXCLUDED {
                    union.pop_last();
                }
                Some(Self::Ne(union))
            }
        }
    }

    /// Returns the disjunction, or `None` if it is not representable.
    fn join(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::Eq(a), Self::Eq(b)) => (a == b).then(|| self.clone()),
            (Self::Eq(a), Self::Ne(set)) | (Self::Ne(set), Self::Eq(a)) => {
                let mut set = set.clone();
                set.remove(a);
                (!set.is_empty()).then_some(Self::Ne(set))
            }
            (Self::Ne(a), Self::Ne(b)) => {
                let both = a.intersection(b).copied().collect::<BTreeSet<_>>();
                (!both.is_empty()).then_some(Self::Ne(both))
            }
        }
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq(value) => write!(f, "== {}", DisplayWord(*value)),
            Self::Ne(values) => {
                write!(f, "!= ")?;
                for (i, value) in values.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayWord(*value))?;
                }
                Ok(())
            }
        }
    }
}

/// How much untracked storage may have changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Clobber {
    /// Untracked slots still hold their entry values.
    #[default]
    None,
    /// Writes through storage-pointer parameters may have changed untracked slots; callers
    /// resolve them through the instantiated write paths.
    Relative,
    /// Unknown writes may have changed every untracked slot.
    All,
}

/// An external call instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallId {
    /// The function containing the call.
    pub(crate) func: FunctionId,
    /// The call instruction.
    pub(crate) inst: InstId,
}

/// A conjunction of entry constraints and caller checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Guard {
    /// Constraints on entry fields.
    pub(crate) conds: BTreeMap<(SlotKey, U256), Constraint>,
    /// Origins `caller` equals.
    pub(crate) caller_is: BTreeSet<Origin>,
}

impl Guard {
    /// Weakens `self` to what both guards imply.
    pub(crate) fn weaken(&mut self, other: &Self) {
        self.conds = std::mem::take(&mut self.conds)
            .into_iter()
            .filter_map(|(key, constraint)| {
                let theirs = other.conds.get(&key)?;
                constraint.join(theirs).map(|constraint| (key, constraint))
            })
            .collect();
        self.caller_is.retain(|origin| other.caller_is.contains(origin));
    }

    /// Displays the guard, or nothing when it is trivially true.
    pub(crate) fn display(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            let mut first = true;
            for ((slot, mask), constraint) in &self.conds {
                f.write_str(if first { "" } else { " && " })?;
                first = false;
                if *mask == U256::MAX {
                    write!(f, "entry({slot}) {constraint}")?;
                } else {
                    write!(f, "entry({slot}) & {mask:#x} {constraint}")?;
                }
            }
            for origin in &self.caller_is {
                f.write_str(if first { "" } else { " && " })?;
                first = false;
                write!(f, "caller == {origin}")?;
            }
            Ok(())
        })
    }
}

/// The abstract state of exact slots at one program point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlotState {
    /// Current words of written slots.
    pub(crate) slots: BTreeMap<SlotKey, SymWord>,
    /// How much untracked storage may have changed.
    pub(crate) clobber: Clobber,
    /// Must facts about entry values and `caller`.
    pub(crate) guard: Guard,
    /// Facts about SSA values.
    pub(crate) values: FxHashMap<ValueId, Val>,
    /// External calls that may have executed before this point.
    pub(crate) calls: BTreeSet<CallId>,
}

impl SlotState {
    /// Returns the current word of `slot`.
    pub(crate) fn current(&self, slot: SlotKey) -> SymWord {
        match self.slots.get(&slot) {
            Some(word) => *word,
            None if self.clobber == Clobber::None => {
                // An entry constraint that fixes the whole slot also fixes its value.
                match self.guard.conds.get(&(slot, U256::MAX)) {
                    Some(Constraint::Eq(value)) => SymWord::constant(*value),
                    _ => SymWord::entry(slot),
                }
            }
            None => SymWord::UNKNOWN,
        }
    }

    /// Records a write of `word` to `slot`.
    pub(crate) fn write(&mut self, slot: SlotKey, word: SymWord) {
        self.slots.insert(slot, word);
    }

    /// Marks every untracked and tracked slot as possibly changed.
    pub(crate) fn clobber(&mut self, level: Clobber) {
        if level == Clobber::None {
            return;
        }
        for word in self.slots.values_mut() {
            *word = SymWord::UNKNOWN;
        }
        self.clobber = self.clobber.max(level);
    }

    /// Assumes `pred`. Returns `false` when it contradicts the state.
    #[must_use]
    pub(crate) fn assume(&mut self, pred: Pred) -> bool {
        match pred {
            Pred::Const(value) => value,
            Pred::Caller { origin, eq } => {
                if eq {
                    self.guard.caller_is.insert(origin);
                }
                true
            }
            Pred::Field { slot, mask, value, eq } => {
                let current = self.current(slot);
                if current.known & mask == mask {
                    return (current.value & mask == value) == eq;
                }
                if !current.is_entry_identity(slot, mask) {
                    return true;
                }
                let constraint =
                    if eq { Constraint::Eq(value) } else { Constraint::Ne([value].into()) };
                let key = (slot, mask);
                let merged = match self.guard.conds.get(&key) {
                    Some(existing) => match existing.meet(&constraint) {
                        Some(merged) => merged,
                        None => return false,
                    },
                    None => constraint,
                };
                // Constraints on other masks of the same slot must stay compatible.
                if let Constraint::Eq(fixed) = merged {
                    for (&(other_slot, other_mask), other) in &self.guard.conds {
                        if other_slot == slot
                            && other_mask != mask
                            && other_mask & mask == other_mask
                            && !other.admits(fixed & other_mask)
                        {
                            return false;
                        }
                    }
                }
                self.guard.conds.insert(key, merged.clone());
                if let Constraint::Eq(fixed) = merged {
                    let mut word = current;
                    word.known |= mask;
                    word.value = (word.value & !mask) | fixed;
                    self.slots.insert(slot, word.normalize());
                }
                true
            }
        }
    }

    /// Returns what is known about `value`.
    pub(crate) fn value(&self, func: &crate::mir::Function, value: ValueId) -> Option<Val> {
        if let Some(constant) = func.value_u256(value) {
            return Some(Val::Word(SymWord::constant(constant)));
        }
        self.values.get(&value).copied()
    }
}

impl JoinSemiLattice for SlotState {
    fn join(&mut self, other: &Self) -> bool {
        let before = self.clone();
        let keys = self.slots.keys().chain(other.slots.keys()).copied().collect::<BTreeSet<_>>();
        let mut slots = BTreeMap::new();
        for key in keys {
            let mut word = self.current(key);
            word.join(&other.current(key));
            slots.insert(key, word);
        }
        self.clobber = self.clobber.max(other.clobber);
        // Drop entries that restate the entry value.
        slots.retain(|&key, word| !(self.clobber == Clobber::None && *word == SymWord::entry(key)));
        self.slots = slots;
        self.guard.weaken(&other.guard);
        for (&value, &fact) in &other.values {
            match self.values.get(&value) {
                Some(&existing) => match existing.join(fact) {
                    Some(joined) => {
                        self.values.insert(value, joined);
                    }
                    None => {
                        self.values.remove(&value);
                    }
                },
                None => {
                    self.values.insert(value, fact);
                }
            }
        }
        self.calls.extend(other.calls.iter().copied());
        *self != before
    }
}

struct DisplayWord(U256);

impl fmt::Display for DisplayWord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(value) = u64::try_from(self.0)
            && value < 1 << 32
        {
            write!(f, "{value}")
        } else {
            write!(f, "{:#x}", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: SlotKey = SlotKey { transient: false, slot: U256::ZERO };

    #[test]
    fn packed_bool_write_keeps_other_bits() {
        let entry = SymWord::entry(SLOT);
        let cleared = entry.and(SymWord::constant(!U256::from(255)));
        let written = cleared.or(SymWord::constant(U256::from(1)));
        let read = written.and(SymWord::constant(U256::from(255)));
        assert_eq!(read.as_constant(), Some(U256::from(1)));
        let other = written.and(SymWord::constant(!U256::from(255)));
        assert!(other.is_entry_identity(SLOT, !U256::from(255)));
    }

    #[test]
    fn masked_equality_constrains_entry_bits() {
        let field = SymWord::entry(SLOT).and(SymWord::constant(U256::from(255)));
        let pred = word_equality(field, SymWord::constant(U256::ZERO)).unwrap();
        assert_eq!(
            pred,
            Pred::Field { slot: SLOT, mask: U256::from(255), value: U256::ZERO, eq: true }
        );
        let mut state = SlotState::default();
        assert!(state.assume(pred));
        assert_eq!(
            state.current(SLOT).and(SymWord::constant(U256::from(255))).as_constant(),
            Some(U256::ZERO)
        );
        assert!(!state.assume(pred.negate()));
    }

    #[test]
    fn composition_substitutes_caller_values() {
        let callee = SymWord::entry(SLOT).shr(8).and(SymWord::constant(U256::from(255)));
        let caller = SymWord::constant(U256::from(0x1200));
        let composed = callee.compose(|_| caller);
        assert_eq!(composed.as_constant(), Some(U256::from(0x12)));
    }
}
