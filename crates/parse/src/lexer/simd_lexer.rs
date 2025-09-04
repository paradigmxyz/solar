//! Fast lexer functions using lookup tables and vectorized operations.
//!
//! This module provides optimized implementations of common lexing operations.
//! For stable Rust, we use lookup tables and manual loop unrolling.

use super::char_class_table::{is_whitespace_fast, is_id_continue_fast};

/// Bulk whitespace skipping function.
/// 
/// Processes the input slice efficiently, returning the total number
/// of whitespace bytes at the start of the input.
pub(super) fn skip_whitespace_bulk(input: &[u8]) -> usize {
    // Process 8 bytes at a time 
    let mut pos = 0;
    
    // Unroll the loop to process multiple bytes at once
    while pos + 8 <= input.len() {
        let chunk = &input[pos..pos + 8];
        
        // Check each byte in the chunk
        let mut count = 0;
        for &byte in chunk {
            if is_whitespace_fast(byte) {
                count += 1;
            } else {
                return pos + count;
            }
        }
        
        // All 8 bytes were whitespace, continue
        if count == 8 {
            pos += 8;
        } else {
            return pos + count;
        }
    }
    
    // Handle remaining bytes
    while pos < input.len() && is_whitespace_fast(input[pos]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_skip_whitespace_bulk() {
        assert_eq!(skip_whitespace_bulk(b"   abc"), 3);
        assert_eq!(skip_whitespace_bulk(b"\t\n\r abc"), 4);
        assert_eq!(skip_whitespace_bulk(b"abc"), 0);
        
        // Test with long whitespace sequence
        let long_whitespace = " ".repeat(100) + "abc";
        assert_eq!(skip_whitespace_bulk(long_whitespace.as_bytes()), 100);
    }
    
    #[test]
    fn test_parse_identifier_bulk() {
        assert_eq!(parse_identifier_bulk(b"abc123_$ "), 8);
        assert_eq!(parse_identifier_bulk(b"a "), 1);
        assert_eq!(parse_identifier_bulk(b" abc"), 0);
        
        // Test with long identifier
        let long_id = "a".repeat(100) + " ";
        assert_eq!(parse_identifier_bulk(long_id.as_bytes()), 100);
    }
    
    #[test]
    fn test_parse_decimal_digits_bulk() {
        assert_eq!(parse_decimal_digits_bulk(b"123456789a"), 9);
        assert_eq!(parse_decimal_digits_bulk(b"1_2_3_4_5a"), 9);
        assert_eq!(parse_decimal_digits_bulk(b"abc"), 0);
        
        // Test with long digit sequence
        let long_digits = "1".repeat(100) + "a";
        assert_eq!(parse_decimal_digits_bulk(long_digits.as_bytes()), 100);
    }
    
    #[test]
    fn test_parse_hex_digits_bulk() {
        assert_eq!(parse_hex_digits_bulk(b"123abc456DEFg"), 12);
        assert_eq!(parse_hex_digits_bulk(b"1_a_2_B_3g"), 9);
        assert_eq!(parse_hex_digits_bulk(b"xyz"), 0);
    }
    
    #[test]
    fn test_find_non_whitespace() {
        assert_eq!(find_non_whitespace(b"   abc"), Some(3));
        assert_eq!(find_non_whitespace(b"\t\n\r abc"), Some(4));
        assert_eq!(find_non_whitespace(b"abc"), Some(0));
        assert_eq!(find_non_whitespace(b"   "), None);
        
        // Test with long whitespace sequence
        let long_whitespace = " ".repeat(100) + "abc";
        assert_eq!(find_non_whitespace(long_whitespace.as_bytes()), Some(100));
    }
    
    #[test]
    fn test_compatibility_with_scalar() {
        let test_cases = [
            &b"   hello world   "[..],
            &b"\t\n\r\n  "[..],
            &b"identifier123_$"[..],
            &b"123456789"[..],
            &b"abc123def456"[..],
            &b""[..],
            &b" "[..],
            &b"a"[..],
        ];
        
        for &input in &test_cases {
            // Test whitespace functions produce same results
            let bulk_result = skip_whitespace_bulk(input);
            let scalar_result = input.iter()
                .take_while(|&&b| is_whitespace_fast(b))
                .count();
            assert_eq!(bulk_result, scalar_result, "Whitespace mismatch for: {:?}", 
                       std::str::from_utf8(input).unwrap_or("invalid utf8"));
            
            // Test identifier functions produce same results  
            let bulk_id_result = parse_identifier_bulk(input);
            let scalar_id_result = input.iter()
                .take_while(|&&b| is_id_continue_fast(b))
                .count();
            assert_eq!(bulk_id_result, scalar_id_result, "Identifier mismatch for: {:?}",
                       std::str::from_utf8(input).unwrap_or("invalid utf8"));
        }
    }
}