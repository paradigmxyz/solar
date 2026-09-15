use super::CandidateRanges;
use std::ops::Range;

fn check_queries(ranges: Vec<Range<usize>>, cursors: impl IntoIterator<Item = usize>) {
    let index = CandidateRanges::new(ranges.clone());
    for cursor in cursors {
        let mut expected =
            ranges.iter().filter(|range| range.contains(&cursor)).cloned().collect::<Vec<_>>();
        let mut actual = index.at(cursor);
        expected.sort_unstable_by_key(|range| (range.start, range.end));
        expected.dedup();
        actual.sort_unstable_by_key(|range| (range.start, range.end));
        actual.dedup();
        assert_eq!(actual, expected, "candidate ranges at byte {cursor}");
    }
}

#[test]
fn queries_preserve_overlaps_and_half_open_boundaries() {
    check_queries(vec![5..10, 0..5, 5..10, 3..8, 2..8, 2..7, 8..12, 0..12], 0..=13);
}

#[test]
fn queries_handle_empty_indexes_and_extreme_offsets() {
    check_queries(Vec::new(), [0, 1, usize::MAX]);
    check_queries(
        vec![0..1, usize::MAX - 1..usize::MAX],
        [0, 1, usize::MAX - 2, usize::MAX - 1, usize::MAX],
    );
}

#[test]
fn mixed_ranges_match_linear_queries_at_every_byte() {
    let mut ranges = std::iter::once(0..320).collect::<Vec<_>>();
    ranges.extend((0..128).map(|index| {
        let start = index * 37 % 256;
        let length = index * 17 % 61 + 1;
        start..start + length
    }));
    ranges.extend([0..320, 100..220, 120..240, 100..180]);

    for len in 0..=ranges.len() {
        check_queries(ranges[..len].to_vec(), 0..=321);
    }
    ranges.reverse();
    check_queries(ranges, 0..=321);
}

#[test]
fn disjoint_groups_and_crossing_ranges_match_linear_queries() {
    let mut ranges = [0, 1000, 2000, 3000]
        .into_iter()
        .flat_map(|base| {
            (0..70).map(move |offset| {
                let start = base + offset * 4;
                start..start + 2
            })
        })
        .collect::<Vec<_>>();
    check_queries(ranges.clone(), 0..=3300);

    ranges[20] = 50..1250;
    ranges[90] = 850..2400;
    ranges[200] = 1900..3100;
    check_queries(ranges.clone(), 0..=3300);
    ranges.reverse();
    check_queries(ranges, 0..=3300);
}

#[test]
fn broad_outer_ranges_preserve_distant_inner_ranges() {
    let count = CandidateRanges::BLOCK_SIZE * 65;
    let end = count * 4;
    let mut ranges = vec![0..end, 0..end / 2, end / 4..end];
    ranges.extend((0..count).map(|index| index * 4..index * 4 + 2));
    let cursors = [
        0,
        1,
        2,
        end / 4,
        end / 2 - 1,
        end / 2,
        end / 2 + 1,
        end - 4,
        end - 3,
        end - 2,
        end - 1,
        end,
        usize::MAX,
    ];
    check_queries(ranges.clone(), cursors);
    ranges.reverse();
    check_queries(ranges, cursors);
}

#[test]
fn equal_block_starts_preserve_all_matching_ranges() {
    let mut ranges =
        (0..CandidateRanges::BLOCK_SIZE * 9).map(|index| 5..5 + index % 17).collect::<Vec<_>>();
    ranges.extend([0..0, 8..8, 3..25]);
    check_queries(ranges.clone(), 0..=26);
    ranges.reverse();
    check_queries(ranges, 0..=26);
}
