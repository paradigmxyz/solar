//! Fast lexer functions using lookup tables and vectorized operations.
//!
//! This module provides optimized implementations of common lexing operations.
//! For stable Rust, we use lookup tables and manual loop unrolling.

use wide::u8x16;

use super::char_class_table::{is_id_continue_fast, is_whitespace_fast};

const LANES: usize = 16;

/// Bulk whitespace skipping function.
///
/// Processes the input slice efficiently, returning the total number
/// of whitespace bytes at the start of the input.
pub(super) fn skip_whitespace_bulk(input: &[u8]) -> usize {
    let mut pos = 0;
    
    while pos + LANES <= input.len() {
        let chunk = u8x16::from(&input[pos..pos + LANES]);

        // True parallel whitespace detection - all 16 bytes compared simultaneously
        let is_space = chunk.cmp_eq(u8x16::splat(b' '));
        let is_tab = chunk.cmp_eq(u8x16::splat(b'\t'));
        let is_newline = chunk.cmp_eq(u8x16::splat(b'\n'));
        let is_carriage = chunk.cmp_eq(u8x16::splat(b'\r'));

        // Combine all whitespace checks with parallel OR operations
        let is_whitespace = is_space | is_tab | is_newline | is_carriage;

        if is_whitespace.all() {
            // All 16 bytes are whitespace - continue
            pos += LANES;
        } else {
            // Found non-whitespace - find exact position
            let mask = is_whitespace.move_mask();
            let idx = mask.trailing_ones() as usize;
            return pos + idx;
        }
    }

    while pos < input.len() && super::char_class_table::is_whitespace_fast(input[pos]) {
        pos += 1;
    }

    pos
}

/// Bulk identifier parsing function.
///
/// Processes the input slice efficiently, returning the total length
/// of identifier-continue characters at the start of the input.
pub(super) fn parse_identifier_bulk(input: &[u8]) -> usize {
    // Process 8 bytes at a time for better performance
    let mut pos = 0;

    // Unroll the loop to process multiple bytes at once
    while pos + 8 <= input.len() {
        let chunk = &input[pos..pos + 8];

        // Check each byte in the chunk
        let mut count = 0;
        for &byte in chunk {
            if is_id_continue_fast(byte) {
                count += 1;
            } else {
                return pos + count;
            }
        }

        // All 8 bytes were identifier chars, continue
        if count == 8 {
            pos += 8;
        } else {
            return pos + count;
        }
    }

    // Handle remaining bytes
    while pos < input.len() && is_id_continue_fast(input[pos]) {
        pos += 1;
    }

    pos
}

/// Fast decimal digit parsing.
pub(super) fn parse_decimal_digits_bulk(input: &[u8]) -> usize {
    let mut pos = 0;

    // Process 8 bytes at a time
    while pos + 8 <= input.len() {
        let chunk = &input[pos..pos + 8];

        let mut count = 0;
        for &byte in chunk {
            if byte.is_ascii_digit() || byte == b'_' {
                count += 1;
            } else {
                return pos + count;
            }
        }

        if count == 8 {
            pos += 8;
        } else {
            return pos + count;
        }
    }

    // Handle remaining bytes
    while pos < input.len() {
        let byte = input[pos];
        if byte.is_ascii_digit() || byte == b'_' {
            pos += 1;
        } else {
            break;
        }
    }

    pos
}

/// Hexadecimal digit parsing.
pub(super) fn parse_hex_digits_bulk(input: &[u8]) -> usize {
    let mut pos = 0;

    // Process 8 bytes at a time
    while pos + 8 <= input.len() {
        let chunk = &input[pos..pos + 8];

        let mut count = 0;
        for &byte in chunk {
            if byte.is_ascii_hexdigit() || byte == b'_' {
                count += 1;
            } else {
                return pos + count;
            }
        }

        if count == 8 {
            pos += 8;
        } else {
            return pos + count;
        }
    }

    // Handle remaining bytes
    while pos < input.len() {
        let byte = input[pos];
        if byte.is_ascii_hexdigit() || byte == b'_' {
            pos += 1;
        } else {
            break;
        }
    }

    pos
}

/// Find the position of the first non-whitespace byte.
#[allow(dead_code)]
pub(super) fn find_non_whitespace(input: &[u8]) -> Option<usize> {
    // Process 8 bytes at a time for better cache efficiency
    let mut pos = 0;

    while pos + 8 <= input.len() {
        let chunk = &input[pos..pos + 8];

        // Check each byte in the chunk
        for (i, &byte) in chunk.iter().enumerate() {
            if !is_whitespace_fast(byte) {
                return Some(pos + i);
            }
        }

        pos += 8;
    }

    // Handle remaining bytes
    while pos < input.len() {
        if !is_whitespace_fast(input[pos]) {
            return Some(pos);
        }
        pos += 1;
    }

    None
}
