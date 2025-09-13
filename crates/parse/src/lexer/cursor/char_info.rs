/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace(c: char) -> bool {
    is_whitespace_byte(ch2u8(c))
}
/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace_byte(c: u8) -> bool {
    classify(c) & WHITESPACE != 0
}

/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start(c: char) -> bool {
    is_id_start_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start_byte(c: u8) -> bool {
    classify(c) & ID_START != 0
}

/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue(c: char) -> bool {
    is_id_continue_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue_byte(c: u8) -> bool {
    classify(c) & ID_CONTINUE != 0
}

#[inline]
pub(super) const fn is_decimal_digit(c: u8) -> bool {
    // classify(c) & DECIMAL_DIGIT != 0
    c.is_ascii_digit()
}

#[inline]
pub(super) const fn is_hex_digit(c: u8) -> bool {
    classify(c) & HEX_DIGIT != 0
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
const DECIMAL_DIGIT: u8 = 1 << 3;
const HEX_DIGIT: u8 = 1 << 4;

#[inline(always)]
const fn classify(c: u8) -> u8 {
    INFO[c as usize]
}

static INFO: [u8; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = classify_impl(i as u8);
        i += 1;
    }
    table
};

const fn classify_impl(c: u8) -> u8 {
    // https://github.com/argotorg/solidity/blob/965166317bbc2b02067eb87f222a2dce9d24e289/liblangutil/Common.h#L20-L46

    let mut result = 0;
    if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
        result |= WHITESPACE;
    }
    if matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$') {
        result |= ID_START;
    }
    if matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' | b'0'..=b'9') {
        result |= ID_CONTINUE;
    }
    if c.is_ascii_digit() {
        result |= DECIMAL_DIGIT;
    }
    if c.is_ascii_hexdigit() {
        result |= HEX_DIGIT;
    }
    result
}
