use num_bigint::BigInt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    Bool,
    Uint(u16),
    /// `bytesN`, as its N*8-bit value (the bytes read as a big-endian number, as storage holds it).
    Bytes(u16),
    Int(u16),
    Addr,
    Contract(String),
    Enum(String, usize),
    Struct(String),
    Map(Box<Self>, Box<Self>),
    /// element type and, for a fixed-size array whose size we could evaluate, its size
    Arr(Box<Self>, Option<u64>),
    /// strings, bytes and anything we do not reason about precisely
    Opaque,
    /// an internal function pointer: its parameter and return types. Its value is 0 when unset,
    /// else the symbolic tag of a function referenced as a value (`Vc::fn_tag`).
    Fn(Box<(Vec<Self>, Vec<Self>)>),
    /// integer literal / mathematical integer (spec mode)
    Lit,
}

pub(crate) fn pow2(n: u32) -> BigInt {
    BigInt::from(1) << n
}

pub(crate) fn num(b: &BigInt) -> String {
    if b.sign() == num_bigint::Sign::Minus { format!("(- {})", -b) } else { b.to_string() }
}

impl Ty {
    pub(crate) fn sort(&self) -> String {
        match self {
            Self::Bool => "Bool".into(),
            // A key keeps its own sort: a `bool` key is a `Bool` term.
            Self::Map(k, v) => format!("(Array {} {})", k.sort(), v.sort()),
            Self::Arr(e, _) => format!("(Array Int {})", e.sort()),
            Self::Struct(n) => format!("|S.{n}|"),
            _ => "Int".into(),
        }
    }
    /// The size of a fixed-size array type.
    pub(crate) fn fixed_len(&self) -> Option<u64> {
        match self {
            Self::Arr(_, n) => *n,
            _ => None,
        }
    }
    pub(crate) fn bounds(&self) -> Option<(BigInt, BigInt)> {
        match self {
            Self::Uint(n) | Self::Bytes(n) => Some((BigInt::from(0), pow2(*n as u32) - 1)),
            Self::Int(n) => Some((-pow2(*n as u32 - 1), pow2(*n as u32 - 1) - 1)),
            Self::Addr | Self::Contract(_) => Some((BigInt::from(0), pow2(160) - 1)),
            Self::Enum(_, k) => Some((BigInt::from(0), BigInt::from(*k as i64 - 1))),
            _ => None,
        }
    }
    pub(crate) fn range(&self, t: &str) -> Option<String> {
        let (lo, hi) = self.bounds()?;
        Some(format!("(and (<= {} {t}) (<= {t} {}))", num(&lo), num(&hi)))
    }
    pub(crate) fn zero(&self) -> String {
        match self {
            Self::Bool => "false".into(),
            Self::Map(_, v) => format!("((as const {}) {})", self.sort(), v.zero()),
            Self::Arr(e, _) => format!("((as const {}) {})", self.sort(), e.zero()),
            Self::Struct(n) => format!("|S.{n}.zero|"),
            _ => "0".into(),
        }
    }
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Bool => "bool".into(),
            Self::Uint(n) => format!("uint{n}"),
            Self::Bytes(n) => format!("bytes{}", n / 8),
            Self::Int(n) => format!("int{n}"),
            Self::Addr => "address".into(),
            Self::Contract(c) => c.clone(),
            Self::Enum(e, _) => e.clone(),
            Self::Struct(n) => n.clone(),
            Self::Map(k, v) => format!("mapping({} => {})", k.name(), v.name()),
            Self::Arr(e, n) => match n {
                Some(n) => format!("{}[{n}]", e.name()),
                None => format!("{}[]", e.name()),
            },
            Self::Opaque => "opaque".into(),
            Self::Fn(sig) => {
                let names = |ts: &[Self]| ts.iter().map(Self::name).collect::<Vec<_>>().join(",");
                if sig.1.is_empty() {
                    format!("function({})", names(&sig.0))
                } else {
                    format!("function({}) returns ({})", names(&sig.0), names(&sig.1))
                }
            }
            Self::Lit => "int".into(),
        }
    }
}

/// Parse a type written in spec text (quantifier binders, ghost declarations).
pub(crate) fn parse_ty_text(s: &str) -> Option<Ty> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("mapping") {
        let inner = rest.trim().strip_prefix('(')?.strip_suffix(')')?;
        let (k, v) = inner.split_once("=>")?;
        return Some(Ty::Map(Box::new(parse_ty_text(k)?), Box::new(parse_ty_text(v)?)));
    }
    if let Some(e) = s.strip_suffix("[]") {
        return Some(Ty::Arr(Box::new(parse_ty_text(e)?), None));
    }
    Some(match s {
        "bool" => Ty::Bool,
        "address" | "address payable" => Ty::Addr,
        "uint" => Ty::Uint(256),
        "int" => Ty::Int(256),
        "nat" => Ty::Uint(65535),
        "integer" | "mathint" => Ty::Lit,
        _ => {
            if let Some(n) = s.strip_prefix("uint") {
                Ty::Uint(n.parse().ok()?)
            } else if let Some(n) = s.strip_prefix("int") {
                Ty::Int(n.parse().ok()?)
            } else {
                let n = s.strip_prefix("bytes")?;
                Ty::Bytes(n.parse::<u16>().ok()? * 8)
            }
        }
    })
}
