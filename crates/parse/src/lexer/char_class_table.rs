//! Character classification lookup table for SIMD lexing.
//!
//! This module provides a pre-computed lookup table that classifies each ASCII character
//! into categories useful for lexical analysis. Instead of using multiple branches per
//! character, we can use a single table lookup or SIMD shuffle operation.

/// Character classification flags
const WHITESPACE: u8 = 0b0000_0001;
const ID_START: u8   = 0b0000_0010;
const ID_CONTINUE: u8= 0b0000_0100;
const DIGIT: u8      = 0b0000_1000;
const HEX_DIGIT: u8  = 0b0001_0000;
const OPERATOR: u8   = 0b0010_0000;
const QUOTE: u8      = 0b0100_0000;
// Comments, delimiters, etc.
const SPECIAL: u8    = 0b1000_0000; 

/// Pre-computed character classification table.
/// 
/// Each byte contains flags indicating which categories the corresponding ASCII character
/// belongs to. For example, 'a' would have ID_START | ID_CONTINUE | HEX_DIGIT flags.
const CHAR_CLASS_TABLE: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0;
    
    while i < 256 {
        let c = i as u8;
        let mut flags = 0u8;
        
        // Whitespace: space, tab, newline, carriage return
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
            flags |= WHITESPACE;
        }
        
        // Identifier start: letters, underscore, dollar sign
        if matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$') {
            flags |= ID_START | ID_CONTINUE;
        }
        
        // Digits
        if matches!(c, b'0'..=b'9') {
            flags |= DIGIT | ID_CONTINUE | HEX_DIGIT;
        }
        
        // Hex digits (letters)
        if matches!(c, b'a'..=b'f' | b'A'..=b'F') {
            flags |= HEX_DIGIT;
        }
        
        // String quotes
        if matches!(c, b'"' | b'\'') {
            flags |= QUOTE;
        }
        
        // Operators and punctuation
        if matches!(c, b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'&' | b'|' 
                     | b'=' | b'!' | b'<' | b'>' | b'~' | b'?' | b':') {
            flags |= OPERATOR;
        }
        
        // Special characters (comments, delimiters, etc.)
        if matches!(c, b'(' | b')' | b'{' | b'}' | b'[' | b']' | b';' | b',' | b'.') {
            flags |= SPECIAL;
        }
        
        table[i] = flags;
        i += 1;
    }
    
    table
};

/// Fast character classification using lookup table.
#[inline]
const fn classify_byte(c: u8) -> u8 {
    CHAR_CLASS_TABLE[c as usize]
}

/// Check if character is whitespace using lookup table.
#[inline]
pub(super) const fn is_whitespace_fast(c: u8) -> bool {
    (classify_byte(c) & WHITESPACE) != 0
}

/// Check if character can start an identifier using lookup table.
#[allow(dead_code)]
#[inline]
pub(super) const fn is_id_start_fast(c: u8) -> bool {
    (classify_byte(c) & ID_START) != 0
}

/// Check if character can continue an identifier using lookup table.
#[inline]
pub(super) const fn is_id_continue_fast(c: u8) -> bool {
    (classify_byte(c) & ID_CONTINUE) != 0
}

/// Check if character is a decimal digit using lookup table.
#[allow(dead_code)]
#[inline]
pub(super) const fn is_digit_fast(c: u8) -> bool {
    (classify_byte(c) & DIGIT) != 0
}

/// Check if character is a hexadecimal digit using lookup table.
#[allow(dead_code)]
#[inline]
pub(super) const fn is_hex_digit_fast(c: u8) -> bool {
    (classify_byte(c) & HEX_DIGIT) != 0
}
