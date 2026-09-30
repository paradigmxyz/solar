//! Symbolic storage locations with field, index, and key sensitivity.
//!
//! Solidity storage addresses are words computed from declared slots by three
//! constructions: a mapping entry hashes its key with the mapping's slot, a dynamic array's
//! data area hashes the array's slot, and struct fields and array elements add constant or
//! scaled offsets. A [`PathNode`] records that construction instead of the resulting word,
//! so two accesses can be compared structurally: `m[k].a` and `m[k].b` differ by their field
//! offset, `xs[i]` and `xs[j]` alias only if their indices may be equal, and `m1[k]` never
//! aliases `m2[k]`. A storage pointer passed to an internal function becomes the formal
//! [`PathNode::Param`], which callers substitute with their actual paths, so a summary
//! describes writes through `S storage s` for every caller at once.
//!
//! Paths are interned in a [`PathTable`] shared by all functions of a module, so equal
//! constructions share one [`PathId`]. A value may denote one of several paths, for
//! example after `c ? a : b`; a [`PathSet`] keeps up to [`MAX_PATHS`] alternatives and
//! degrades to [`PathNode::Unknown`], which aliases every slot.
//!
//! Alias queries assume Keccak-256 is collision free and that hashed locations lie far from
//! both the low absolute slots and each other, the same assumptions solc's storage layout
//! makes. Offsets added to a location are assumed not to wrap around the slot space; the
//! frontend's bounds checks establish this for array indices. Keys are compared per
//! [`Activation`]: within one function activation the same SSA value denotes the same word,
//! while accesses from different transactions or reentrant calls only share constants.

use crate::mir::{ArgIdx, Function, FunctionId, ValueId, analysis::AliasResult};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{index::IndexVec, map::FxHashMap, newtype_index};
use std::fmt::{self, Display as _};

use super::lattice::JoinSemiLattice;

/// Maximum alternatives in one [`PathSet`] before it widens to the unknown path.
pub(crate) const MAX_PATHS: usize = 4;

/// Maximum nesting depth of a path before it widens to the unknown path.
const MAX_DEPTH: usize = 12;

newtype_index! {
    /// An interned storage path.
    pub(crate) struct PathId;
}

/// A symbolic mapping key or array index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum KeyTerm {
    /// A known constant word.
    Const(U256),
    /// `msg.sender` of the current activation.
    Caller,
    /// A formal parameter of the summarized function.
    Arg(ArgIdx),
    /// An SSA value of one function.
    Local(FunctionId, ValueId),
    /// An unknown word that may equal any key.
    Any,
}

/// How to compare keys that are not constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Activation {
    /// Both accesses belong to the same function activation.
    Same,
    /// The accesses belong to different activations, such as a reentrant call.
    Different,
    /// The accesses belong to transactions from different senders, so `caller` keys differ.
    DifferentSenders,
}

/// One symbolic storage location construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PathNode {
    /// An absolute slot.
    Slot(U256),
    /// The storage pointer passed as a formal parameter.
    Param(ArgIdx),
    /// `keccak256(key . base)`: a mapping entry.
    Mapping {
        /// The mapping's slot.
        base: PathId,
        /// The entry's key.
        key: KeyTerm,
    },
    /// `keccak256(base)`: the data area of a dynamic array or long byte array.
    ArrayData {
        /// The array's slot.
        base: PathId,
    },
    /// `base + index * stride`: one element of an array area.
    Element {
        /// The array area's first slot.
        base: PathId,
        /// The element index.
        index: KeyTerm,
        /// Slots per element.
        stride: u64,
    },
    /// `base + offset`: a struct field or a constant array slot.
    Field {
        /// The aggregate's first slot.
        base: PathId,
        /// Constant slot offset.
        offset: U256,
    },
    /// Every slot derived from `base`, such as a byte array's header and data.
    Region {
        /// The root slot of the region.
        base: PathId,
    },
    /// A location the analysis cannot describe; aliases every slot.
    Unknown,
}

/// Canonical decomposition of a path into a root and an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Root {
    Absolute,
    Hash(PathId),
    Param(ArgIdx),
    Region(PathId),
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Offset {
    constant: U256,
    terms: SmallVec<[(KeyTerm, u64); 1]>,
}

/// Interns storage paths for one module.
#[derive(Clone, Debug)]
pub(crate) struct PathTable {
    nodes: IndexVec<PathId, PathNode>,
    ids: FxHashMap<PathNode, PathId>,
    depths: IndexVec<PathId, usize>,
}

impl Default for PathTable {
    fn default() -> Self {
        let mut table =
            Self { nodes: IndexVec::new(), ids: FxHashMap::default(), depths: IndexVec::new() };
        let unknown = table.push(PathNode::Unknown, 0);
        debug_assert_eq!(unknown, Self::UNKNOWN);
        table
    }
}

impl PathTable {
    /// The unknown path.
    pub(crate) const UNKNOWN: PathId = PathId::new(0);

    /// Returns the interned id of `node`, if it has one.
    pub(crate) fn find(&self, node: PathNode) -> Option<PathId> {
        self.ids.get(&node).copied()
    }

    /// Interns `node` after canonicalizing constant offsets.
    pub(crate) fn intern(&mut self, node: PathNode) -> PathId {
        let node = match node {
            PathNode::Field { offset, base } if offset.is_zero() => return base,
            PathNode::Field { base, offset } => match self.nodes[base] {
                PathNode::Slot(slot) => PathNode::Slot(slot.wrapping_add(offset)),
                PathNode::Field { base, offset: inner } => {
                    PathNode::Field { base, offset: inner.wrapping_add(offset) }
                }
                PathNode::Unknown => return Self::UNKNOWN,
                _ => node,
            },
            PathNode::Element { base, index: KeyTerm::Const(index), stride } => {
                let offset = index.wrapping_mul(U256::from(stride));
                return self.intern(PathNode::Field { base, offset });
            }
            PathNode::Mapping { base, .. }
            | PathNode::ArrayData { base }
            | PathNode::Element { base, .. }
            | PathNode::Region { base }
                if base == Self::UNKNOWN =>
            {
                return Self::UNKNOWN;
            }
            _ => node,
        };
        if let Some(&id) = self.ids.get(&node) {
            return id;
        }
        let depth = match node {
            PathNode::Mapping { base, .. }
            | PathNode::ArrayData { base }
            | PathNode::Element { base, .. }
            | PathNode::Field { base, .. }
            | PathNode::Region { base } => self.depths[base] + 1,
            PathNode::Slot(_) | PathNode::Param(_) | PathNode::Unknown => 0,
        };
        if depth > MAX_DEPTH {
            return Self::UNKNOWN;
        }
        self.push(node, depth)
    }

    fn push(&mut self, node: PathNode, depth: usize) -> PathId {
        let id = self.nodes.push(node);
        self.depths.push(depth);
        self.ids.insert(node, id);
        id
    }

    /// Returns the path of an absolute slot.
    pub(crate) fn slot(&mut self, slot: U256) -> PathId {
        self.intern(PathNode::Slot(slot))
    }

    /// Returns the path of formal storage-pointer parameter `index`.
    pub(crate) fn param(&mut self, index: ArgIdx) -> PathId {
        self.intern(PathNode::Param(index))
    }

    /// Returns whether `path` refers to a formal parameter or a callee-local key.
    pub(crate) fn is_relative(&self, path: PathId) -> bool {
        match self.nodes[path] {
            PathNode::Param(_) => true,
            PathNode::Slot(_) | PathNode::Unknown => false,
            PathNode::Mapping { base, key } => {
                matches!(key, KeyTerm::Arg(_) | KeyTerm::Local(..)) || self.is_relative(base)
            }
            PathNode::Element { base, index, .. } => {
                matches!(index, KeyTerm::Arg(_) | KeyTerm::Local(..)) || self.is_relative(base)
            }
            PathNode::ArrayData { base }
            | PathNode::Field { base, .. }
            | PathNode::Region { base } => self.is_relative(base),
        }
    }

    /// Substitutes formal parameters and keys with a caller's actual values.
    ///
    /// `args[i]` gives the paths of the storage pointer passed as parameter `i`, and
    /// `keys[i]` the key term of the same argument. Keys local to the callee become
    /// [`KeyTerm::Any`] because the caller cannot name them.
    pub(crate) fn instantiate(
        &mut self,
        path: PathId,
        args: &[PathSet],
        keys: &[KeyTerm],
    ) -> PathSet {
        let node = self.nodes[path];
        let key = |key: KeyTerm| match key {
            KeyTerm::Arg(index) => keys.get(index.index()).copied().unwrap_or(KeyTerm::Any),
            KeyTerm::Local(..) => KeyTerm::Any,
            key => key,
        };
        let rebuild = |this: &mut Self, base: PathId, build: &dyn Fn(PathId) -> PathNode| {
            let bases = this.instantiate(base, args, keys);
            PathSet::from_iter(bases.iter().map(|base| this.intern(build(base))))
        };
        match node {
            PathNode::Slot(_) | PathNode::Unknown => PathSet::single(path),
            PathNode::Param(index) => {
                args.get(index.index()).cloned().unwrap_or_else(PathSet::unknown)
            }
            PathNode::Mapping { base, key: map_key } => {
                let map_key = key(map_key);
                rebuild(self, base, &|base| PathNode::Mapping { base, key: map_key })
            }
            PathNode::ArrayData { base } => {
                rebuild(self, base, &|base| PathNode::ArrayData { base })
            }
            PathNode::Element { base, index, stride } => {
                let index = key(index);
                rebuild(self, base, &|base| PathNode::Element { base, index, stride })
            }
            PathNode::Field { base, offset } => {
                rebuild(self, base, &|base| PathNode::Field { base, offset })
            }
            PathNode::Region { base } => rebuild(self, base, &|base| PathNode::Region { base }),
        }
    }

    /// Replaces keys that only `func` can name with [`KeyTerm::Any`].
    pub(crate) fn generalize(&mut self, path: PathId, func: FunctionId) -> PathId {
        let generalize = |key: KeyTerm| match key {
            KeyTerm::Local(owner, _) if owner == func => KeyTerm::Any,
            key => key,
        };
        match self.nodes[path] {
            PathNode::Slot(_) | PathNode::Param(_) | PathNode::Unknown => path,
            PathNode::Mapping { base, key } => {
                let base = self.generalize(base, func);
                self.intern(PathNode::Mapping { base, key: generalize(key) })
            }
            PathNode::ArrayData { base } => {
                let base = self.generalize(base, func);
                self.intern(PathNode::ArrayData { base })
            }
            PathNode::Element { base, index, stride } => {
                let base = self.generalize(base, func);
                self.intern(PathNode::Element { base, index: generalize(index), stride })
            }
            PathNode::Field { base, offset } => {
                let base = self.generalize(base, func);
                self.intern(PathNode::Field { base, offset })
            }
            PathNode::Region { base } => {
                let base = self.generalize(base, func);
                self.intern(PathNode::Region { base })
            }
        }
    }

    /// Replaces every key except constants and `caller` with [`KeyTerm::Any`], describing
    /// all instances of `path` over any activation.
    pub(crate) fn erase_keys(&mut self, path: PathId) -> PathId {
        let erase = |key: KeyTerm| match key {
            KeyTerm::Const(_) | KeyTerm::Caller => key,
            _ => KeyTerm::Any,
        };
        match self.nodes[path] {
            PathNode::Slot(_) | PathNode::Param(_) | PathNode::Unknown => path,
            PathNode::Mapping { base, key } => {
                let base = self.erase_keys(base);
                self.intern(PathNode::Mapping { base, key: erase(key) })
            }
            PathNode::ArrayData { base } => {
                let base = self.erase_keys(base);
                self.intern(PathNode::ArrayData { base })
            }
            PathNode::Element { base, index, stride } => {
                let base = self.erase_keys(base);
                self.intern(PathNode::Element { base, index: erase(index), stride })
            }
            PathNode::Field { base, offset } => {
                let base = self.erase_keys(base);
                self.intern(PathNode::Field { base, offset })
            }
            PathNode::Region { base } => {
                let base = self.erase_keys(base);
                self.intern(PathNode::Region { base })
            }
        }
    }

    /// Returns whether `path` names one slot whose keys cannot change within an activation:
    /// constants, `caller`, and formal parameters, with no unknown parts.
    pub(crate) fn is_stable(&self, path: PathId) -> bool {
        let stable =
            |key: KeyTerm| matches!(key, KeyTerm::Const(_) | KeyTerm::Caller | KeyTerm::Arg(_));
        match self.nodes[path] {
            PathNode::Slot(_) | PathNode::Param(_) => true,
            PathNode::Unknown | PathNode::Region { .. } => false,
            PathNode::Mapping { base, key } => stable(key) && self.is_stable(base),
            PathNode::Element { base, index, .. } => stable(index) && self.is_stable(base),
            PathNode::ArrayData { base } | PathNode::Field { base, .. } => self.is_stable(base),
        }
    }

    /// Returns whether `path` lies in a hashed area: a mapping entry or array data.
    pub(crate) fn is_hashed(&self, path: PathId) -> bool {
        match self.nodes[path] {
            PathNode::Mapping { .. } | PathNode::ArrayData { .. } => true,
            PathNode::Field { base, .. }
            | PathNode::Element { base, .. }
            | PathNode::Region { base } => self.is_hashed(base),
            PathNode::Slot(_) | PathNode::Param(_) | PathNode::Unknown => false,
        }
    }

    /// Returns the constant slot of `path`, if it is absolute.
    pub(crate) fn as_slot(&self, path: PathId) -> Option<U256> {
        match self.nodes[path] {
            PathNode::Slot(slot) => Some(slot),
            _ => None,
        }
    }

    /// Compares two paths.
    pub(crate) fn alias(&self, a: PathId, b: PathId, activation: Activation) -> AliasResult {
        if a == b && !self.has_unknown(a) && (activation == Activation::Same || !self.has_keys(a)) {
            return AliasResult::MustAlias;
        }
        let (root_a, offset_a) = self.decompose(a);
        let (root_b, offset_b) = self.decompose(b);
        match (&root_a, &root_b) {
            (Root::Unknown, _) | (_, Root::Unknown) => AliasResult::MayAlias,
            (&Root::Region(base), _) => self.region_alias(base, b, activation),
            (_, &Root::Region(base)) => self.region_alias(base, a, activation),
            (Root::Param(x), Root::Param(y)) if x == y => {
                Self::offset_alias(&offset_a, &offset_b, activation)
            }
            (Root::Param(_), _) | (_, Root::Param(_)) => AliasResult::MayAlias,
            (Root::Absolute, Root::Absolute) => {
                Self::offset_alias(&offset_a, &offset_b, activation)
            }
            (Root::Absolute, Root::Hash(_)) | (Root::Hash(_), Root::Absolute) => {
                AliasResult::NoAlias
            }
            (&Root::Hash(x), &Root::Hash(y)) => match self.hash_alias(x, y, activation) {
                AliasResult::NoAlias => AliasResult::NoAlias,
                AliasResult::MustAlias => Self::offset_alias(&offset_a, &offset_b, activation),
                AliasResult::MayAlias | AliasResult::PartialAlias => {
                    // Distinct hashes are far apart, so definitely different offsets from
                    // either one or two roots never meet.
                    match Self::offset_alias(&offset_a, &offset_b, activation) {
                        AliasResult::NoAlias => AliasResult::NoAlias,
                        _ => AliasResult::MayAlias,
                    }
                }
            },
        }
    }

    /// Returns whether two paths may denote the same slot.
    pub(crate) fn may_alias(&self, a: PathId, b: PathId, activation: Activation) -> bool {
        self.alias(a, b, activation).may_alias()
    }

    fn has_unknown(&self, path: PathId) -> bool {
        match self.nodes[path] {
            PathNode::Unknown => true,
            PathNode::Slot(_) | PathNode::Param(_) => false,
            PathNode::Mapping { base, key } => key == KeyTerm::Any || self.has_unknown(base),
            PathNode::Element { base, index, .. } => {
                index == KeyTerm::Any || self.has_unknown(base)
            }
            PathNode::ArrayData { base }
            | PathNode::Field { base, .. }
            | PathNode::Region { base } => self.has_unknown(base),
        }
    }

    fn has_keys(&self, path: PathId) -> bool {
        match self.nodes[path] {
            PathNode::Unknown | PathNode::Slot(_) | PathNode::Param(_) => false,
            PathNode::Mapping { base, key } => {
                !matches!(key, KeyTerm::Const(_)) || self.has_keys(base)
            }
            PathNode::Element { base, index, .. } => {
                !matches!(index, KeyTerm::Const(_)) || self.has_keys(base)
            }
            PathNode::ArrayData { base }
            | PathNode::Field { base, .. }
            | PathNode::Region { base } => self.has_keys(base),
        }
    }

    fn decompose(&self, path: PathId) -> (Root, Offset) {
        match self.nodes[path] {
            PathNode::Slot(slot) => {
                (Root::Absolute, Offset { constant: slot, terms: SmallVec::new() })
            }
            PathNode::Param(index) => {
                (Root::Param(index), Offset { constant: U256::ZERO, terms: SmallVec::new() })
            }
            PathNode::Mapping { .. } | PathNode::ArrayData { .. } => {
                (Root::Hash(path), Offset { constant: U256::ZERO, terms: SmallVec::new() })
            }
            PathNode::Region { base } => {
                (Root::Region(base), Offset { constant: U256::ZERO, terms: SmallVec::new() })
            }
            PathNode::Unknown => {
                (Root::Unknown, Offset { constant: U256::ZERO, terms: SmallVec::new() })
            }
            PathNode::Field { base, offset } => {
                let (root, mut inner) = self.decompose(base);
                inner.constant = inner.constant.wrapping_add(offset);
                (root, inner)
            }
            PathNode::Element { base, index, stride } => {
                let (root, mut inner) = self.decompose(base);
                inner.terms.push((index, stride));
                (root, inner)
            }
        }
    }

    fn key_alias(a: KeyTerm, b: KeyTerm, activation: Activation) -> AliasResult {
        match (a, b) {
            (KeyTerm::Const(x), KeyTerm::Const(y)) => {
                if x == y {
                    AliasResult::MustAlias
                } else {
                    AliasResult::NoAlias
                }
            }
            (KeyTerm::Any, _) | (_, KeyTerm::Any) => AliasResult::MayAlias,
            (KeyTerm::Caller, KeyTerm::Caller) if activation == Activation::DifferentSenders => {
                AliasResult::NoAlias
            }
            _ if activation == Activation::Same && a == b => AliasResult::MustAlias,
            _ => AliasResult::MayAlias,
        }
    }

    fn hash_alias(&self, a: PathId, b: PathId, activation: Activation) -> AliasResult {
        match (self.nodes[a], self.nodes[b]) {
            (PathNode::Mapping { base: x, key: kx }, PathNode::Mapping { base: y, key: ky }) => {
                let keys = Self::key_alias(kx, ky, activation);
                if keys == AliasResult::NoAlias {
                    return AliasResult::NoAlias;
                }
                match self.alias(x, y, activation) {
                    AliasResult::NoAlias => AliasResult::NoAlias,
                    AliasResult::MustAlias if keys == AliasResult::MustAlias => {
                        AliasResult::MustAlias
                    }
                    _ => AliasResult::MayAlias,
                }
            }
            (PathNode::ArrayData { base: x }, PathNode::ArrayData { base: y }) => {
                match self.alias(x, y, activation) {
                    AliasResult::PartialAlias => AliasResult::MayAlias,
                    result => result,
                }
            }
            // Mapping entries hash two words while array data hashes one.
            _ => AliasResult::NoAlias,
        }
    }

    fn offset_alias(a: &Offset, b: &Offset, activation: Activation) -> AliasResult {
        let same_terms = a.terms.len() == b.terms.len()
            && a.terms.iter().zip(&b.terms).all(|(&(ka, sa), &(kb, sb))| {
                sa == sb && Self::key_alias(ka, kb, activation) == AliasResult::MustAlias
            });
        if same_terms {
            return if a.constant == b.constant {
                AliasResult::MustAlias
            } else {
                AliasResult::NoAlias
            };
        }
        // Elements of the same stride never overlap in different fields of one element.
        let strides_match = a.terms.len() == b.terms.len()
            && a.terms.iter().zip(&b.terms).all(|(&(_, sa), &(_, sb))| sa == sb);
        if strides_match
            && let Some(&(_, stride)) = a.terms.last()
            && stride > 1
        {
            let stride = U256::from(stride);
            if a.constant < stride && b.constant < stride && a.constant != b.constant {
                return AliasResult::NoAlias;
            }
        }
        if a.terms.is_empty() && b.terms.is_empty() {
            return AliasResult::NoAlias;
        }
        AliasResult::MayAlias
    }

    fn region_alias(&self, base: PathId, other: PathId, activation: Activation) -> AliasResult {
        let mut current = Some(other);
        while let Some(path) = current {
            if self.may_alias_non_region(path, base, activation) {
                return AliasResult::MayAlias;
            }
            current = match self.nodes[path] {
                PathNode::Mapping { base, .. }
                | PathNode::ArrayData { base }
                | PathNode::Element { base, .. }
                | PathNode::Field { base, .. }
                | PathNode::Region { base } => Some(base),
                PathNode::Slot(_) | PathNode::Param(_) | PathNode::Unknown => None,
            };
        }
        AliasResult::NoAlias
    }

    fn may_alias_non_region(&self, a: PathId, b: PathId, activation: Activation) -> bool {
        match (self.nodes[a], self.nodes[b]) {
            (PathNode::Region { base }, _) => self.may_alias_non_region(base, b, activation),
            (_, PathNode::Region { base }) => self.may_alias_non_region(a, base, activation),
            _ => self.may_alias(a, b, activation),
        }
    }

    /// Displays `path`, naming local keys through `func` when given.
    pub(crate) fn display<'a>(
        &'a self,
        path: PathId,
        func: Option<&'a Function>,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| self.fmt_path(f, path, func))
    }

    fn fmt_path(
        &self,
        f: &mut fmt::Formatter<'_>,
        path: PathId,
        func: Option<&Function>,
    ) -> fmt::Result {
        match self.nodes[path] {
            PathNode::Slot(slot) => write!(f, "slot({})", DisplayWord(slot)),
            PathNode::Param(index) => write!(f, "arg{}", index.index()),
            PathNode::Mapping { base, key } => {
                self.fmt_path(f, base, func)?;
                write!(f, "[{}]", DisplayKey(key, func))
            }
            PathNode::ArrayData { base } => {
                write!(f, "data(")?;
                self.fmt_path(f, base, func)?;
                write!(f, ")")
            }
            PathNode::Element { base, index, stride } => {
                self.fmt_path(f, base, func)?;
                if stride == 1 {
                    write!(f, "<{}>", DisplayKey(index, func))
                } else {
                    write!(f, "<{} x{stride}>", DisplayKey(index, func))
                }
            }
            PathNode::Field { base, offset } => {
                self.fmt_path(f, base, func)?;
                write!(f, ".{}", DisplayWord(offset))
            }
            PathNode::Region { base } => {
                write!(f, "region(")?;
                self.fmt_path(f, base, func)?;
                write!(f, ")")
            }
            PathNode::Unknown => write!(f, "?"),
        }
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

struct DisplayKey<'a>(KeyTerm, Option<&'a Function>);

impl fmt::Display for DisplayKey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            KeyTerm::Const(value) => DisplayWord(value).fmt(f),
            KeyTerm::Caller => f.write_str("caller"),
            KeyTerm::Arg(index) => write!(f, "arg{}", index.index()),
            KeyTerm::Local(_, value) => match self.1 {
                Some(func) => crate::mir::display::display_val(value, func).fmt(f),
                None => f.write_str("*"),
            },
            KeyTerm::Any => f.write_str("*"),
        }
    }
}

/// The paths a storage-pointer value may denote.
///
/// The empty set is bottom: no value has reached this point yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct PathSet(SmallVec<[PathId; 2]>);

impl PathSet {
    /// A set with one path.
    pub(crate) fn single(path: PathId) -> Self {
        Self(smallvec::smallvec![path])
    }

    /// The set containing only the unknown path.
    pub(crate) fn unknown() -> Self {
        Self::single(PathTable::UNKNOWN)
    }

    /// Returns whether the set is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns whether the set contains the unknown path.
    pub(crate) fn is_unknown(&self) -> bool {
        self.0.contains(&PathTable::UNKNOWN)
    }

    /// Iterates over the alternatives in ascending order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = PathId> + '_ {
        self.0.iter().copied()
    }

    /// Returns the only path, if there is exactly one.
    pub(crate) fn as_single(&self) -> Option<PathId> {
        match self.0.as_slice() {
            &[path] => Some(path),
            _ => None,
        }
    }

    /// Adds one alternative. Returns whether the set changed.
    pub(crate) fn insert(&mut self, path: PathId) -> bool {
        if self.is_unknown() {
            return false;
        }
        if path == PathTable::UNKNOWN || self.0.len() == MAX_PATHS && !self.0.contains(&path) {
            self.0.clear();
            self.0.push(PathTable::UNKNOWN);
            return true;
        }
        match self.0.binary_search(&path) {
            Ok(_) => false,
            Err(index) => {
                self.0.insert(index, path);
                true
            }
        }
    }

    /// Displays the set.
    pub(crate) fn display<'a>(
        &'a self,
        table: &'a PathTable,
        func: Option<&'a Function>,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            if let Some(path) = self.as_single() {
                return table.display(path, func).fmt(f);
            }
            write!(f, "{{")?;
            for (i, path) in self.iter().enumerate() {
                if i != 0 {
                    write!(f, ", ")?;
                }
                table.display(path, func).fmt(f)?;
            }
            write!(f, "}}")
        })
    }
}

impl FromIterator<PathId> for PathSet {
    fn from_iter<I: IntoIterator<Item = PathId>>(iter: I) -> Self {
        let mut set = Self::default();
        for path in iter {
            set.insert(path);
        }
        set
    }
}

impl JoinSemiLattice for PathSet {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for path in other.iter() {
            changed |= self.insert(path);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(table: &mut PathTable, base: PathId, key: KeyTerm) -> PathId {
        table.intern(PathNode::Mapping { base, key })
    }

    fn field(table: &mut PathTable, base: PathId, offset: U256) -> PathId {
        table.intern(PathNode::Field { base, offset })
    }

    #[test]
    fn fields_of_one_mapping_entry_are_disjoint() {
        let mut table = PathTable::default();
        let base = table.slot(U256::from(3));
        let a = mapping(&mut table, base, KeyTerm::Any);
        let b = mapping(&mut table, base, KeyTerm::Any);
        let a0 = field(&mut table, a, U256::ZERO);
        let b1 = field(&mut table, b, U256::from(1));
        assert_eq!(table.alias(a0, b1, Activation::Same), AliasResult::NoAlias);
        assert_eq!(table.alias(a0, b, Activation::Same), AliasResult::MayAlias);
    }

    #[test]
    fn distinct_mappings_and_constant_keys_are_disjoint() {
        let mut table = PathTable::default();
        let m1 = table.slot(U256::from(1));
        let m2 = table.slot(U256::from(2));
        let caller = KeyTerm::Caller;
        let a = mapping(&mut table, m1, caller);
        let b = mapping(&mut table, m2, caller);
        assert_eq!(table.alias(a, b, Activation::Same), AliasResult::NoAlias);
        let c = mapping(&mut table, m1, KeyTerm::Const(U256::from(1)));
        let d = mapping(&mut table, m1, KeyTerm::Const(U256::from(2)));
        assert_eq!(table.alias(c, d, Activation::Different), AliasResult::NoAlias);
        assert_eq!(table.alias(a, a, Activation::Same), AliasResult::MustAlias);
        assert_eq!(table.alias(a, a, Activation::Different), AliasResult::MayAlias);
        assert_eq!(table.alias(a, m1, Activation::Same), AliasResult::NoAlias);
    }

    #[test]
    fn instantiation_substitutes_parameters_and_keys() {
        let mut table = PathTable::default();
        let param = table.param(ArgIdx::from_usize(0));
        let entry = mapping(&mut table, param, KeyTerm::Arg(ArgIdx::from_usize(1)));
        let relative = field(&mut table, entry, U256::from(2));
        let actual = table.slot(U256::from(9));
        let result = table.instantiate(
            relative,
            &[PathSet::single(actual), PathSet::default()],
            &[KeyTerm::Any, KeyTerm::Caller],
        );
        let expected_entry = mapping(&mut table, actual, KeyTerm::Caller);
        let expected = field(&mut table, expected_entry, U256::from(2));
        assert_eq!(result, PathSet::single(expected));
    }

    #[test]
    fn regions_cover_derived_paths_only() {
        let mut table = PathTable::default();
        let bytes = table.slot(U256::from(4));
        let region = table.intern(PathNode::Region { base: bytes });
        let data = table.intern(PathNode::ArrayData { base: bytes });
        let other = table.slot(U256::from(5));
        assert!(table.may_alias(region, data, Activation::Same));
        assert!(table.may_alias(region, bytes, Activation::Same));
        assert!(!table.may_alias(region, other, Activation::Same));
    }

    #[test]
    fn path_sets_widen_to_unknown() {
        let mut table = PathTable::default();
        let mut set = PathSet::default();
        for slot in 0..MAX_PATHS as u64 {
            assert!(set.insert(table.slot(U256::from(slot))));
        }
        assert!(!set.is_unknown());
        assert!(set.insert(table.slot(U256::from(100))));
        assert!(set.is_unknown());
    }
}
