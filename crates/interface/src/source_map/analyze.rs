use super::MultiByteChar;
use crate::pos::RelativeBytePos;
use solar_data_structures::hint::cold_path;

/// Finds all newlines, multi-byte characters, and non-narrow characters in a
/// SourceFile.
///
/// This function will use an SSE2 enhanced implementation if hardware support
/// is detected at runtime.
pub(crate) fn analyze_source_file(src: &str) -> (Vec<RelativeBytePos>, Vec<MultiByteChar>) {
    let mut lines = vec![RelativeBytePos::from_u32(0)];
    let mut multi_byte_chars = vec![];

    // Calls the right implementation, depending on hardware support available.
    analyze_source_file_dispatch(src, &mut lines, &mut multi_byte_chars);

    // The code above optimistically registers a new line *after* each \n
    // it encounters. If that point is already outside the source_file, remove
    // it again.
    if let Some(&last_line_start) = lines.last() {
        let source_file_end = RelativeBytePos::from_usize(src.len());
        assert!(source_file_end >= last_line_start);
        if last_line_start == source_file_end {
            lines.pop();
        }
    }

    (lines, multi_byte_chars)
}

fn analyze_source_file_dispatch(
    src: &str,
    lines: &mut Vec<RelativeBytePos>,
    multi_byte_chars: &mut Vec<MultiByteChar>,
) {
    #[cfg(any(feature = "nightly", target_arch = "x86", target_arch = "x86_64"))]
    'b: {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let enabled = is_x86_feature_detected!("sse2");
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        let enabled = true;
        if !enabled {
            break 'b;
        }
        unsafe { analyze_source_file_vectorized(src, lines, multi_byte_chars) };
        return;
    }
    analyze_source_file_generic(
        src,
        src.len(),
        RelativeBytePos::from_u32(0),
        lines,
        multi_byte_chars,
    );
}

#[cfg(any(feature = "nightly", target_arch = "x86", target_arch = "x86_64"))]
#[cfg_attr(not(feature = "nightly"), target_feature(enable = "sse2"))]
unsafe fn analyze_source_file_vectorized(
    src: &str,
    lines: &mut Vec<RelativeBytePos>,
    multi_byte_chars: &mut Vec<MultiByteChar>,
) {
    #[cfg(feature = "nightly")]
    mod imp {
        use std::simd::prelude::*;

        pub(super) const CHUNK_SIZE: usize = if cfg!(target_feature = "avx512f") {
            64
        } else if cfg!(target_feature = "avx2") {
            32
        } else {
            16
        };
        pub(super) type Chunk = std::simd::Simd<u8, CHUNK_SIZE>;

        pub(super) fn load_chunk(chunk_bytes: &[u8; CHUNK_SIZE]) -> Chunk {
            Chunk::from_array(*chunk_bytes)
        }

        pub(super) fn is_ascii(chunk: Chunk) -> bool {
            chunk.simd_lt(Chunk::splat(128)).all()
        }

        pub(super) fn new_line_mask(chunk: Chunk) -> u64 {
            chunk.simd_eq(Chunk::splat(b'\n')).to_bitmask()
        }
    }

    #[cfg(not(feature = "nightly"))]
    mod imp {
        #[cfg(target_arch = "x86")]
        use std::arch::x86::*;
        #[cfg(target_arch = "x86_64")]
        use std::arch::x86_64::*;

        pub(super) const CHUNK_SIZE: usize = 16;
        pub(super) type Chunk = __m128i;

        pub(super) fn load_chunk(chunk_bytes: &[u8; CHUNK_SIZE]) -> Chunk {
            unsafe { _mm_loadu_si128(chunk_bytes.as_ptr() as *const __m128i) }
        }

        pub(super) fn is_ascii(chunk: Chunk) -> bool {
            unsafe {
                let test = _mm_cmplt_epi8(chunk, _mm_set1_epi8(0));
                let mask = _mm_movemask_epi8(test);
                mask == 0
            }
        }

        pub(super) fn new_line_mask(chunk: Chunk) -> u64 {
            unsafe {
                let test = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'\n' as i8));
                _mm_movemask_epi8(test) as u64
            }
        }
    }

    use imp::*;

    let (chunks, tail) = src.as_bytes().as_chunks::<CHUNK_SIZE>();

    // This variable keeps track of where we should start decoding a
    // chunk. If a multi-byte character spans across chunk boundaries,
    // we need to skip that part in the next chunk because we already
    // handled it.
    let mut intra_chunk_offset = 0;

    for (chunk_index, chunk) in chunks.iter().enumerate() {
        let chunk = load_chunk(chunk);
        if is_ascii(chunk) {
            debug_assert_eq!(intra_chunk_offset, 0);
            let output_offset = RelativeBytePos::from_usize(chunk_index * CHUNK_SIZE + 1);
            let mut mask = new_line_mask(chunk);
            while mask != 0 {
                let i = mask.trailing_zeros() as usize;
                lines.push(output_offset + RelativeBytePos::from_usize(i));
                mask &= mask - 1;
            }
        } else {
            cold_path();
            // The slow path.
            // There are multibyte chars in here, fallback to generic decoding.
            let scan_start = chunk_index * CHUNK_SIZE + intra_chunk_offset;
            intra_chunk_offset = analyze_source_file_generic(
                &src[scan_start..],
                CHUNK_SIZE - intra_chunk_offset,
                RelativeBytePos::from_usize(scan_start),
                lines,
                multi_byte_chars,
            );
        }
    }

    // There might still be a tail left to analyze
    let tail_start = src.len() - tail.len() + intra_chunk_offset;
    if tail_start < src.len() {
        analyze_source_file_generic(
            &src[tail_start..],
            src.len() - tail_start,
            RelativeBytePos::from_usize(tail_start),
            lines,
            multi_byte_chars,
        );
    }
}

// `scan_len` determines the number of bytes in `src` to scan. Note that the
// function can read past `scan_len` if a multi-byte character start within the
// range but extends past it. The overflow is returned by the function.
fn analyze_source_file_generic(
    src: &str,
    scan_len: usize,
    output_offset: RelativeBytePos,
    lines: &mut Vec<RelativeBytePos>,
    multi_byte_chars: &mut Vec<MultiByteChar>,
) -> usize {
    assert!(src.len() >= scan_len);
    let mut i = 0;
    let src_bytes = src.as_bytes();

    while i < scan_len {
        let byte = unsafe {
            // We verified that i < scan_len <= src.len()
            *src_bytes.get_unchecked(i)
        };

        // How much to advance in order to get to the next UTF-8 char in the
        // string.
        let mut char_len = 1;

        if byte == b'\n' {
            let pos = RelativeBytePos::from_usize(i) + output_offset;
            lines.push(pos + RelativeBytePos(1));
        } else if byte >= 128 {
            // This is the beginning of a multibyte char. Just decode to `char`.
            let c = src[i..].chars().next().unwrap();
            char_len = c.len_utf8();

            let pos = RelativeBytePos::from_usize(i) + output_offset;
            assert!((2..=4).contains(&char_len));
            let mbc = MultiByteChar { pos, bytes: char_len as u8 };
            multi_byte_chars.push(mbc);
        }

        i += char_len;
    }

    i - scan_len
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test {
        (
            case:
            $test_name:ident,text:
            $text:expr,lines:
            $lines:expr,multi_byte_chars:
            $multi_byte_chars:expr,
        ) => {
            #[test]
            fn $test_name() {
                let (lines, multi_byte_chars) = analyze_source_file($text);

                let expected_lines: Vec<RelativeBytePos> =
                    $lines.into_iter().map(RelativeBytePos).collect();

                assert_eq!(lines, expected_lines);

                let expected_mbcs: Vec<MultiByteChar> = $multi_byte_chars
                    .into_iter()
                    .map(|(pos, bytes)| MultiByteChar { pos: RelativeBytePos(pos), bytes })
                    .collect();

                assert_eq!(multi_byte_chars, expected_mbcs);
            }
        };
    }

    test!(
        case: empty_text,
        text: "",
        lines: vec![],
        multi_byte_chars: vec![],
    );

    test!(
        case: newlines_short,
        text: "a\nc",
        lines: vec![0, 2],
        multi_byte_chars: vec![],
    );

    test!(
        case: newlines_long,
        text: "012345678\nabcdef012345678\na",
        lines: vec![0, 10, 26],
        multi_byte_chars: vec![],
    );

    test!(
        case: newline_and_multi_byte_char_in_same_chunk,
        text: "01234β789\nbcdef0123456789abcdef",
        lines: vec![0, 11],
        multi_byte_chars: vec![(5, 2)],
    );

    test!(
        case: newline_and_control_char_in_same_chunk,
        text: "01234\u{07}6789\nbcdef0123456789abcdef",
        lines: vec![0, 11],
        multi_byte_chars: vec![],
    );

    test!(
        case: multi_byte_char_short,
        text: "aβc",
        lines: vec![0],
        multi_byte_chars: vec![(1, 2)],
    );

    test!(
        case: multi_byte_char_long,
        text: "0123456789abcΔf012345β",
        lines: vec![0],
        multi_byte_chars: vec![(13, 2), (22, 2)],
    );

    test!(
        case: multi_byte_char_across_chunk_boundary,
        text: "0123456789abcdeΔ123456789abcdef01234",
        lines: vec![0],
        multi_byte_chars: vec![(15, 2)],
    );

    test!(
        case: multi_byte_char_across_chunk_boundary_tail,
        text: "0123456789abcdeΔ....",
        lines: vec![0],
        multi_byte_chars: vec![(15, 2)],
    );

    test!(
        case: non_narrow_short,
        text: "0\t2",
        lines: vec![0],
        multi_byte_chars: vec![],
    );

    test!(
        case: non_narrow_long,
        text: "01\t3456789abcdef01234567\u{07}9",
        lines: vec![0],
        multi_byte_chars: vec![],
    );

    test!(
        case: output_offset_all,
        text: "01\t345\n789abcΔf01234567\u{07}9\nbcΔf",
        lines: vec![0, 7, 27],
        multi_byte_chars: vec![(13, 2), (29, 2)],
    );
}
