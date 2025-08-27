use super::Box;
use semver::Op;
use solar_data_structures::smallvec::{SmallVec, smallvec};
use solar_interface::Span;
use std::{cmp::Ordering, fmt};

pub use semver::Op as SemverOp;

// We use the same approach as Solc.
// See [`SemverReq::dis`] field docs for more details on how the requirements are treated.

// Solc implementation notes:
// - uses `unsigned` (`u32`) for version integers: https://github.com/argotorg/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L258
// - version numbers can be `*/x/X`, which are represented as `u32::MAX`: https://github.com/argotorg/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L263
// - ranges are parsed as `>=start, <=end`: https://github.com/argotorg/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L209
//   we however dedicate a separate node for this: [`SemverReqComponentKind::Range`]

/// A SemVer version number.
#[derive(Clone, Copy)]
pub enum SemverVersionNumber {
    /// A number.
    Number(u32),
    /// `*`, `X`, or `x`.
    Wildcard,
}

impl From<u64> for SemverVersionNumber {
    #[inline]
    fn from(n: u64) -> Self {
        match n.try_into() {
            Ok(n) => Self::Number(n),
            Err(_) => Self::Wildcard,
        }
    }
}

impl From<SemverVersionNumber> for u64 {
    #[inline]
    fn from(value: SemverVersionNumber) -> Self {
        match value {
            SemverVersionNumber::Number(n) => n as Self,
            SemverVersionNumber::Wildcard => Self::MAX,
        }
    }
}

impl fmt::Display for SemverVersionNumber {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => n.fmt(f),
            Self::Wildcard => "*".fmt(f),
        }
    }
}

impl fmt::Debug for SemverVersionNumber {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl PartialEq for SemverVersionNumber {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Wildcard, _) | (_, Self::Wildcard) => true,
            (Self::Number(a), Self::Number(b)) => a == b,
        }
    }
}

impl Eq for SemverVersionNumber {}

impl PartialOrd for SemverVersionNumber {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemverVersionNumber {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Wildcard, _) | (_, Self::Wildcard) => Ordering::Equal,
            (Self::Number(a), Self::Number(b)) => a.cmp(b),
        }
    }
}

/// A SemVer version.
#[derive(Clone)]
pub struct SemverVersion {
    pub span: Span,
    /// Major version.
    pub major: SemverVersionNumber,
    /// Minor version. Optional.
    pub minor: Option<SemverVersionNumber>,
    /// Patch version. Optional.
    pub patch: Option<SemverVersionNumber>,
    // Pre-release and build metadata are not supported.
}

impl PartialEq for SemverVersion {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for SemverVersion {}

impl PartialOrd for SemverVersion {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemverVersion {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        #[inline]
        fn cmp_opt(a: &Option<SemverVersionNumber>, b: &Option<SemverVersionNumber>) -> Ordering {
            match (a, b) {
                (Some(a), Some(b)) => a.cmp(b),
                _ => Ordering::Equal,
            }
        }

        self.major
            .cmp(&other.major)
            .then_with(|| cmp_opt(&self.minor, &other.minor))
            .then_with(|| cmp_opt(&self.patch, &other.patch))
    }
}

impl fmt::Display for SemverVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { span: _, major, minor, patch } = *self;
        write!(f, "{major}")?;
        if let Some(minor) = minor {
            write!(f, ".{minor}")?;
        }
        if let Some(patch) = patch {
            if minor.is_none() {
                f.write_str(".*")?;
            }
            write!(f, ".{patch}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for SemverVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemverVersion")
            .field("span", &self.span)
            .field("version", &format_args!("{self}"))
            .finish()
    }
}

impl From<semver::Version> for SemverVersion {
    #[inline]
    fn from(version: semver::Version) -> Self {
        Self {
            span: Span::DUMMY,
            major: version.major.into(),
            minor: Some(version.minor.into()),
            patch: Some(version.patch.into()),
        }
    }
}

impl From<SemverVersion> for semver::Version {
    #[inline]
    fn from(version: SemverVersion) -> Self {
        Self::new(
            version.major.into(),
            version.minor.map(Into::into).unwrap_or(0),
            version.patch.map(Into::into).unwrap_or(0),
        )
    }
}

impl SemverVersion {
    /// Creates a new [::semver] version from this version.
    #[inline]
    pub fn into_semver(self) -> semver::Version {
        self.into()
    }
}

/// A SemVer version requirement. This is a list of components, and is never empty.
#[derive(Debug)]
pub struct SemverReq<'ast> {
    /// The components of this requirement.
    ///
    /// Or-ed list of and-ed components, meaning that `matches` is evaluated as
    /// `any([all(c) for c in dis])`.
    /// E.g.: `^0 <=1 || 0.5.0 - 0.6.0 ... || ...` -> `[[^0, <=1], [0.5.0 - 0.6.0, ...], ...]`
    pub dis: Box<'ast, [SemverReqCon<'ast>]>,
}

impl fmt::Display for SemverReq<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, con) in self.dis.iter().enumerate() {
            if i > 0 {
                f.write_str(" || ")?;
            }
            write!(f, "{con}")?;
        }
        Ok(())
    }
}

impl SemverReq<'_> {
    /// Returns `true` if the given version satisfies this requirement.
    pub fn matches(&self, version: &SemverVersion) -> bool {
        self.dis.iter().any(|c| c.matches(version))
    }

    /// Converts this requirement to a [::semver] version requirement.
    pub fn to_semver(&self) -> SemverVersionReqCompat {
        SemverVersionReqCompat { reqs: self.dis.iter().map(SemverReqCon::to_semver).collect() }
    }
}

/// A list of or-ed [`semver::VersionReq`].
///
/// Obtained with [`SemverReq::to_semver`].
pub struct SemverVersionReqCompat {
    /// The list of requirements.
    pub reqs: Vec<semver::VersionReq>,
}

impl SemverVersionReqCompat {
    /// Returns `true` if the given version satisfies this requirement.
    pub fn matches(&self, version: &semver::Version) -> bool {
        self.reqs.iter().any(|r| r.matches(version))
    }
}

/// A list of conjoint SemVer version requirement components.
#[derive(Debug)]
pub struct SemverReqCon<'ast> {
    pub span: Span,
    /// The list of components. See [`SemverReq::dis`] for more details.
    pub components: Box<'ast, [SemverReqComponent]>,
}

impl fmt::Display for SemverReqCon<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (j, component) in self.components.iter().enumerate() {
            if j > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{component}")?;
        }
        Ok(())
    }
}

impl SemverReqCon<'_> {
    /// Converts this requirement to a [::semver] version requirement.
    pub fn to_semver(&self) -> semver::VersionReq {
        semver::VersionReq {
            comparators: self.components.iter().flat_map(SemverReqComponent::to_semver).collect(),
        }
    }

    /// Returns `true` if the given version satisfies this requirement.
    pub fn matches(&self, version: &SemverVersion) -> bool {
        self.components.iter().all(|c| c.matches(version))
    }
}

/// A single SemVer version requirement component.
#[derive(Clone, Debug)]
pub struct SemverReqComponent {
    pub span: Span,
    pub kind: SemverReqComponentKind,
}

impl fmt::Display for SemverReqComponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl SemverReqComponent {
    /// Converts this requirement component to a [::semver] comparator.
    pub fn to_semver(&self) -> SmallVec<[semver::Comparator; 2]> {
        self.kind.to_semver()
    }

    /// Returns `true` if the given version satisfies this requirement component.
    pub fn matches(&self, version: &SemverVersion) -> bool {
        self.kind.matches(version)
    }
}

/// A SemVer version requirement component.
#[derive(Clone, Debug)]
pub enum SemverReqComponentKind {
    /// `v`, `=v`
    Op(Option<Op>, SemverVersion),
    /// `l - r`
    Range(SemverVersion, SemverVersion),
}

impl fmt::Display for SemverReqComponentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Op(op, version) => {
                if let Some(op) = op {
                    let op = match op {
                        Op::Exact => "=",
                        Op::Greater => ">",
                        Op::GreaterEq => ">=",
                        Op::Less => "<",
                        Op::LessEq => "<=",
                        Op::Tilde => "~",
                        Op::Caret => "^",
                        Op::Wildcard => "*",
                        _ => "",
                    };
                    f.write_str(op)?;
                }
                write!(f, "{version}")
            }
            Self::Range(left, right) => write!(f, "{left} - {right}"),
        }
    }
}

impl SemverReqComponentKind {
    /// Converts this requirement component to a [::semver] comparator.
    pub fn to_semver(&self) -> SmallVec<[semver::Comparator; 2]> {
        let cvt_op = |op: Option<Op>, version: &SemverVersion| semver::Comparator {
            op: op.unwrap_or(Op::Exact),
            major: version.major.into(),
            minor: version.minor.map(Into::into),
            patch: version.patch.map(Into::into),
            pre: Default::default(),
        };
        match self {
            Self::Op(op, version) => smallvec![cvt_op(*op, version)],
            Self::Range(start, end) => smallvec![
                cvt_op(Some(semver::Op::GreaterEq), start),
                cvt_op(Some(semver::Op::LessEq), end)
            ],
        }
    }

    /// Returns `true` if the given version satisfies this requirement component.
    pub fn matches(&self, version: &SemverVersion) -> bool {
        match self {
            Self::Op(op, other) => matches_op(op.unwrap_or(Op::Exact), version, other),
            Self::Range(start, end) => {
                matches_op(Op::GreaterEq, version, start) && matches_op(Op::LessEq, version, end)
            }
        }
    }
}

fn matches_op(op: Op, a: &SemverVersion, b: &SemverVersion) -> bool {
    match op {
        Op::Exact => a == b,
        Op::Greater => a > b,
        Op::GreaterEq => a >= b,
        Op::Less => a < b,
        Op::LessEq => a <= b,
        Op::Tilde => matches_tilde(a, b),
        Op::Caret => matches_caret(a, b),
        Op::Wildcard => true,
        _ => false,
    }
}

fn matches_tilde(a: &SemverVersion, b: &SemverVersion) -> bool {
    // https://github.com/argotorg/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L80
    if !matches_op(Op::GreaterEq, a, b) {
        return false;
    }

    let mut a = a.clone();
    a.patch = None;
    matches_op(Op::LessEq, &a, b)
}

fn matches_caret(a: &SemverVersion, b: &SemverVersion) -> bool {
    // https://github.com/argotorg/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L95
    if !matches_op(Op::GreaterEq, a, b) {
        return false;
    }

    let mut a = a.clone();
    if a.major > SemverVersionNumber::Number(0) {
        a.minor = None;
    }
    a.patch = None;
    matches_op(Op::LessEq, &a, b)
}

// Tests in `crates/parse/src/parser/item.rs`
