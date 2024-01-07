use semver::Op;
use std::fmt;
use sulk_interface::Span;

pub use semver::Op as SemverOp;

// We use the same approach as Solc.
// See [`SemverReq::dis`] field docs for more details on how the requirements are treated.

// Solc implementation notes:
// - uses `unsigned` (`u32`) for version integers: https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L258
// - version numbers can be `*/x/X`, which are represented as `u32::MAX`: https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L263
// - ranges are parsed as `>=start, <=end`: https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L209
//   we however dedicate a separate node for this: [`SemverReqComponentKind::Range`]

/// A SemVer version. `u32::MAX` values represent `*` (or `x`/`X`, which behaves the same) in source
/// code.
#[derive(Clone, Debug)]
pub struct SemverVersion {
    pub span: Span,
    /// Major version.
    pub major: u32,
    /// Minor version. Optional.
    pub minor: Option<u32>,
    /// Patch version. Optional.
    pub patch: Option<u32>,
    // Pre-release and build metadata are not supported.
}

impl PartialOrd for SemverVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemverVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
    }
}

impl PartialEq for SemverVersion {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch == other.patch
    }
}

impl Eq for SemverVersion {}

impl fmt::Display for SemverVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let write_number = |n: u32, f: &mut fmt::Formatter<'_>| {
            if n == u32::MAX {
                f.write_str("*")
            } else {
                write!(f, "{n}")
            }
        };

        let Self { span: _, major, minor, patch } = *self;
        write_number(major, f)?;
        if let Some(minor) = minor {
            f.write_str(".")?;
            write_number(minor, f)?;
        }
        if let Some(patch) = patch {
            if minor.is_none() {
                f.write_str(".*")?;
            }
            f.write_str(".")?;
            write_number(patch, f)?;
        }
        Ok(())
    }
}

impl From<semver::Version> for SemverVersion {
    fn from(version: semver::Version) -> Self {
        Self {
            span: Span::DUMMY,
            major: version.major.try_into().unwrap_or(u32::MAX),
            minor: Some(version.minor.try_into().unwrap_or(u32::MAX)),
            patch: Some(version.patch.try_into().unwrap_or(u32::MAX)),
        }
    }
}

impl From<SemverVersion> for semver::Version {
    fn from(version: SemverVersion) -> Self {
        Self::new(
            version.major as u64,
            version.minor.unwrap_or(0) as u64,
            version.patch.unwrap_or(0) as u64,
        )
    }
}

impl SemverVersion {
    /// Creates a new version.
    pub const fn new(span: Span, major: u32, minor: Option<u32>, patch: Option<u32>) -> Self {
        Self { span, major, minor, patch }
    }

    /// Creates a new [::semver] version from this version.
    pub fn into_semver(self) -> semver::Version {
        self.into()
    }
}

/// A SemVer version requirement. This is a list of components, and is never empty.
#[derive(Clone, Debug)]
pub struct SemverReq {
    /// The components of this requirement.
    ///
    /// Or-ed list of and-ed components, meaning that `matches` is evaluated as
    /// `any([all(c) for c in dis])`.
    /// E.g.: `^0 <= 1 || 0.5.0 - 0.6.0 ...1 || ...2` -> `[[^0, <=1], [0.5.0 - 0.6.0, ...1], ...2]`
    pub dis: Vec<SemverReqCon>,
}

impl fmt::Display for SemverReq {
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

impl SemverReq {
    /// Returns `true` if the given version satisfies this requirement.
    pub fn matches(&self, version: &SemverVersion) -> bool {
        self.dis.iter().any(|c| c.matches(version))
    }
}

/// A list of conjoint SemVer version requirement components.
#[derive(Clone, Debug)]
pub struct SemverReqCon {
    pub span: Span,
    /// The list of components. See [`SemverReq::dis`] for more details.
    pub components: Vec<SemverReqComponent>,
}

impl fmt::Display for SemverReqCon {
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

impl SemverReqCon {
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
    // https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L80
    if !matches_op(Op::GreaterEq, a, b) {
        return false;
    }

    let mut a = a.clone();
    a.patch = None;
    matches_op(Op::LessEq, &a, b)
}

fn matches_caret(a: &SemverVersion, b: &SemverVersion) -> bool {
    // https://github.com/ethereum/solidity/blob/e81f2bdbd66e9c8780f74b8a8d67b4dc2c87945e/liblangutil/SemVerHandler.cpp#L95
    if !matches_op(Op::GreaterEq, a, b) {
        return false;
    }

    let mut a = a.clone();
    if a.major > 0 {
        a.minor = None;
    }
    a.patch = None;
    matches_op(Op::LessEq, &a, b)
}
