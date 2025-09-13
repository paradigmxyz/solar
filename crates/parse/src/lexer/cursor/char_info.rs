/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace(c: char) -> bool {
    is_whitespace_byte(ch2u8(c))
}
/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace_byte(c: u8) -> bool {
    INFO[c as usize] & WHITESPACE != 0
}

/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start(c: char) -> bool {
    is_id_start_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start_byte(c: u8) -> bool {
    INFO[c as usize] & ID_START != 0
}

/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue(c: char) -> bool {
    is_id_continue_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue_byte(c: u8) -> bool {
    INFO[c as usize] & ID_CONTINUE != 0
}

/// Returns `true` if the given string is a valid Solidity identifier.
///
/// An identifier in Solidity has to start with a letter, a dollar-sign or an underscore and may
/// additionally contain numbers after the first symbol.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.Identifier>
#[inline]
pub const fn is_ident(s: &str) -> bool {
    is_ident_bytes(s.as_bytes())
}

/// Returns `true` if the given byte slice is a valid Solidity identifier.
///
/// See [`is_ident`] for more details.
pub const fn is_ident_bytes(s: &[u8]) -> bool {
    let [first, ref rest @ ..] = *s else {
        return false;
    };

    if !is_id_start_byte(first) {
        return false;
    }

    let mut i = 0;
    while i < rest.len() {
        if !is_id_continue_byte(rest[i]) {
            return false;
        }
        i += 1;
    }

    true
}

/// Converts a `char` to a `u8`.
#[inline(always)]
const fn ch2u8(c: char) -> u8 {
    c as u32 as u8
}

pub(super) const EOF: u8 = b'\0';

const WHITESPACE: u8 = 1 << 0;
const ID_START: u8 = 1 << 1;
const ID_CONTINUE: u8 = 1 << 2;

static INFO: [u8; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = classify(i as u8);
        i += 1;
    }
    table
};

const fn classify(c: u8) -> u8 {
    // https://github.com/argotorg/solidity/blob/965166317bbc2b02067eb87f222a2dce9d24e289/liblangutil/Common.h#L20-L46
    match c {
        b' ' | b'\t' | b'\n' | b'\r' => WHITESPACE,
        b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => ID_START | ID_CONTINUE,
        b'0'..=b'9' => ID_CONTINUE,
        _ => 0,
    }
}
