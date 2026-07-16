#![allow(
    clippy::type_complexity,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::while_let_on_iterator
)]

use super::*;
use crate::{index::Idx, newtype_index};

newtype_index! {
    struct TestIdx;
}

fn idx(index: usize) -> TestIdx {
    <TestIdx as Idx>::from_usize(index)
}

fn idx_range(range: std::ops::Range<usize>) -> std::ops::Range<TestIdx> {
    idx(range.start)..idx(range.end)
}

fn idx_range_inclusive(
    range: std::ops::RangeInclusive<usize>,
) -> std::ops::RangeInclusive<TestIdx> {
    let (start, end) = range.into_inner();
    idx(start)..=idx(end)
}

#[test]
fn test_new_filled() {
    let _ = TestIdx::MAX;
    for i in 0..128 {
        let idx_buf = DenseBitSet::<TestIdx>::new_filled(i);
        let elems: Vec<usize> = idx_buf.iter().map(Idx::index).collect();
        let expected: Vec<usize> = (0..i).collect();
        assert_eq!(elems, expected);
    }
}

#[test]
fn bitset_iter_works() {
    let mut bitset: DenseBitSet<TestIdx> = DenseBitSet::new_empty(100);
    bitset.insert(idx(1));
    bitset.insert(idx(10));
    bitset.insert(idx(19));
    bitset.insert(idx(62));
    bitset.insert(idx(63));
    bitset.insert(idx(64));
    bitset.insert(idx(65));
    bitset.insert(idx(66));
    bitset.insert(idx(99));
    assert_eq!(
        bitset.iter().map(Idx::index).collect::<Vec<_>>(),
        [1, 10, 19, 62, 63, 64, 65, 66, 99]
    );
}

#[test]
fn bitset_iter_works_2() {
    let mut bitset: DenseBitSet<TestIdx> = DenseBitSet::new_empty(320);
    bitset.insert(idx(0));
    bitset.insert(idx(127));
    bitset.insert(idx(191));
    bitset.insert(idx(255));
    bitset.insert(idx(319));
    assert_eq!(bitset.iter().map(Idx::index).collect::<Vec<_>>(), [0, 127, 191, 255, 319]);
}

#[test]
fn bitset_clone_from() {
    let mut a: DenseBitSet<TestIdx> = DenseBitSet::new_empty(10);
    a.insert(idx(4));
    a.insert(idx(7));
    a.insert(idx(9));

    let mut b = DenseBitSet::new_empty(2);
    b.clone_from(&a);
    assert_eq!(b.domain_size(), 10);
    assert_eq!(b.iter().map(Idx::index).collect::<Vec<_>>(), [4, 7, 9]);

    b.clone_from(&DenseBitSet::new_empty(40));
    assert_eq!(b.domain_size(), 40);
    assert_eq!(b.iter().map(Idx::index).collect::<Vec<_>>(), Vec::<usize>::new());
}

#[test]
fn union_two_sets() {
    let mut set1: DenseBitSet<TestIdx> = DenseBitSet::new_empty(65);
    let mut set2: DenseBitSet<TestIdx> = DenseBitSet::new_empty(65);
    assert!(set1.insert(idx(3)));
    assert!(!set1.insert(idx(3)));
    assert!(set2.insert(idx(5)));
    assert!(set2.insert(idx(64)));
    assert!(set1.union(&set2));
    assert!(!set1.union(&set2));
    assert!(set1.contains(idx(3)));
    assert!(!set1.contains(idx(4)));
    assert!(set1.contains(idx(5)));
    assert!(!set1.contains(idx(63)));
    assert!(set1.contains(idx(64)));
}

#[test]
fn union_not() {
    let mut a = DenseBitSet::<TestIdx>::new_empty(100);
    let mut b = DenseBitSet::<TestIdx>::new_empty(100);

    a.insert(idx(3));
    a.insert(idx(5));
    a.insert(idx(80));
    a.insert(idx(81));

    b.insert(idx(5)); // Already in `a`.
    b.insert(idx(7));
    b.insert(idx(63));
    b.insert(idx(81)); // Already in `a`.
    b.insert(idx(90));

    a.union_not(&b);

    // After union-not, `a` should contain all values in the domain, except for
    // the ones that are in `b` and were _not_ already in `a`.
    assert_eq!(
        a.iter().map(Idx::index).collect::<Vec<_>>(),
        (0usize..100).filter(|&x| !matches!(x, 7 | 63 | 90)).collect::<Vec<_>>(),
    );
}

#[test]
fn chunked_bitset() {
    let mut b0 = ChunkedBitSet::<TestIdx>::new_empty(0);
    let b0b = b0.clone();
    assert_eq!(b0, ChunkedBitSet { domain_size: 0, chunks: Box::new([]), marker: PhantomData });

    // There are no valid insert/remove/contains operations on a 0-domain
    // bitset, but we can test `union`.
    b0.assert_valid();
    assert!(!b0.union(&b0b));
    assert_eq!(b0.chunks(), vec![]);
    assert_eq!(b0.count(), 0);
    b0.assert_valid();

    //-----------------------------------------------------------------------

    let mut b1 = ChunkedBitSet::<TestIdx>::new_empty(1);
    assert_eq!(
        b1,
        ChunkedBitSet {
            domain_size: 1,
            chunks: Box::new([Zeros { chunk_domain_size: 1 }]),
            marker: PhantomData
        }
    );

    b1.assert_valid();
    assert!(!b1.contains(idx(0)));
    assert_eq!(b1.count(), 0);
    assert!(b1.insert(idx(0)));
    assert!(b1.contains(idx(0)));
    assert_eq!(b1.count(), 1);
    assert_eq!(b1.chunks(), [Ones { chunk_domain_size: 1 }]);
    assert!(!b1.insert(idx(0)));
    assert!(b1.remove(idx(0)));
    assert!(!b1.contains(idx(0)));
    assert_eq!(b1.count(), 0);
    assert_eq!(b1.chunks(), [Zeros { chunk_domain_size: 1 }]);
    b1.assert_valid();

    //-----------------------------------------------------------------------

    let mut b100 = ChunkedBitSet::<TestIdx>::new_filled(100);
    assert_eq!(
        b100,
        ChunkedBitSet {
            domain_size: 100,
            chunks: Box::new([Ones { chunk_domain_size: 100 }]),
            marker: PhantomData
        }
    );

    b100.assert_valid();
    for i in 0..100 {
        assert!(b100.contains(idx(i)));
    }
    assert_eq!(b100.count(), 100);
    assert!(b100.remove(idx(3)));
    assert!(b100.insert(idx(3)));
    assert_eq!(b100.chunks(), vec![Ones { chunk_domain_size: 100 }]);
    assert!(
        b100.remove(idx(20))
            && b100.remove(idx(30))
            && b100.remove(idx(40))
            && b100.remove(idx(99))
            && b100.insert(idx(30))
    );
    assert_eq!(b100.count(), 97);
    assert!(
        !b100.contains(idx(20))
            && b100.contains(idx(30))
            && !b100.contains(idx(99))
            && b100.contains(idx(50))
    );
    assert_eq!(
        b100.chunks(),
        vec![Mixed {
            chunk_domain_size: 100,
            ones_count: 97,
            words: Rc::new([
                0b11111111_11111111_11111110_11111111_11111111_11101111_11111111_11111111,
                0b00000000_00000000_00000000_00000111_11111111_11111111_11111111_11111111,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ])
        }],
    );
    b100.assert_valid();
    let mut num_removed = 0;
    for i in 0..100 {
        if b100.remove(idx(i)) {
            num_removed += 1;
        }
    }
    assert_eq!(num_removed, 97);
    assert_eq!(b100.chunks(), vec![Zeros { chunk_domain_size: 100 }]);
    b100.assert_valid();

    //-----------------------------------------------------------------------

    let mut b2548 = ChunkedBitSet::<TestIdx>::new_empty(2548);
    assert_eq!(
        b2548,
        ChunkedBitSet {
            domain_size: 2548,
            chunks: Box::new([Zeros { chunk_domain_size: 2048 }, Zeros { chunk_domain_size: 500 }]),
            marker: PhantomData
        }
    );

    b2548.assert_valid();
    b2548.insert(idx(14));
    b2548.remove(idx(14));
    assert_eq!(
        b2548.chunks(),
        vec![Zeros { chunk_domain_size: 2048 }, Zeros { chunk_domain_size: 500 }]
    );
    b2548.insert_all();
    for i in 0..2548 {
        assert!(b2548.contains(idx(i)));
    }
    assert_eq!(b2548.count(), 2548);
    assert_eq!(
        b2548.chunks(),
        vec![Ones { chunk_domain_size: 2048 }, Ones { chunk_domain_size: 500 }]
    );
    b2548.assert_valid();

    //-----------------------------------------------------------------------

    let mut b4096 = ChunkedBitSet::<TestIdx>::new_empty(4096);
    assert_eq!(
        b4096,
        ChunkedBitSet {
            domain_size: 4096,
            chunks: Box::new([
                Zeros { chunk_domain_size: 2048 },
                Zeros { chunk_domain_size: 2048 }
            ]),
            marker: PhantomData
        }
    );

    b4096.assert_valid();
    for i in 0..4096 {
        assert!(!b4096.contains(idx(i)));
    }
    assert!(b4096.insert(idx(0)) && b4096.insert(idx(4095)) && !b4096.insert(idx(4095)));
    assert!(
        b4096.contains(idx(0))
            && !b4096.contains(idx(2047))
            && !b4096.contains(idx(2048))
            && b4096.contains(idx(4095))
    );
    assert_eq!(
        b4096.chunks(),
        vec![
            Mixed {
                chunk_domain_size: 2048,
                ones_count: 1,
                words: Rc::new([
                    1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0
                ])
            },
            Mixed {
                chunk_domain_size: 2048,
                ones_count: 1,
                words: Rc::new([
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0x8000_0000_0000_0000
                ])
            },
        ],
    );
    assert_eq!(b4096.count(), 2);
    b4096.assert_valid();

    //-----------------------------------------------------------------------

    let mut b10000 = ChunkedBitSet::<TestIdx>::new_empty(10000);
    assert_eq!(
        b10000,
        ChunkedBitSet {
            domain_size: 10000,
            chunks: Box::new([
                Zeros { chunk_domain_size: 2048 },
                Zeros { chunk_domain_size: 2048 },
                Zeros { chunk_domain_size: 2048 },
                Zeros { chunk_domain_size: 2048 },
                Zeros { chunk_domain_size: 1808 }
            ]),
            marker: PhantomData,
        }
    );

    b10000.assert_valid();
    assert!(b10000.insert(idx(3000)) && b10000.insert(idx(5000)));
    assert_eq!(
        b10000.chunks(),
        vec![
            Zeros { chunk_domain_size: 2048 },
            Mixed {
                chunk_domain_size: 2048,
                ones_count: 1,
                words: Rc::new([
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0x0100_0000_0000_0000,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ])
            },
            Mixed {
                chunk_domain_size: 2048,
                ones_count: 1,
                words: Rc::new([
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x0100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0,
                ])
            },
            Zeros { chunk_domain_size: 2048 },
            Zeros { chunk_domain_size: 1808 },
        ],
    );
    let mut b10000b = ChunkedBitSet::<TestIdx>::new_empty(10000);
    b10000b.clone_from(&b10000);
    assert_eq!(b10000, b10000b);
    for i in 6000..7000 {
        b10000b.insert(idx(i));
    }
    assert_eq!(b10000b.count(), 1002);
    b10000b.assert_valid();
    b10000b.clone_from(&b10000);
    assert_eq!(b10000b.count(), 2);
    for i in 2000..8000 {
        b10000b.insert(idx(i));
    }
    b10000.union(&b10000b);
    assert_eq!(b10000.count(), 6000);
    b10000.union(&b10000b);
    assert_eq!(b10000.count(), 6000);
    b10000.assert_valid();
    b10000b.assert_valid();

    //-----------------------------------------------------------------------

    let mut b64 = ChunkedBitSet::<TestIdx>::new_filled(64);

    let mut b64b = ChunkedBitSet::<TestIdx>::new_empty(64);
    b64b.insert(idx(0));

    b64.subtract(&b64b);
    assert!(!b64.contains(idx(0)));
    assert!(b64.contains(idx(10)));
    assert!(b64.contains(idx(50)));
    assert!(b64.contains(idx(63)));
    assert_eq!(
        b64.chunks(),
        vec![Mixed {
            chunk_domain_size: 64,
            ones_count: 63,
            words: Rc::new([
                0xfffffffffffffffe,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ])
        },],
    );
}

/// Additional helper methods for testing.
impl ChunkedBitSet<TestIdx> {
    /// Creates a new `ChunkedBitSet` containing all `i` for which `fill_fn(i)` is true.
    fn fill_with(domain_size: usize, fill_fn: impl Fn(usize) -> bool) -> Self {
        let mut this = ChunkedBitSet::new_empty(domain_size);
        for i in 0..domain_size {
            if fill_fn(i) {
                this.insert(idx(i));
            }
        }
        this
    }

    /// Asserts that for each `i` in `0..self.domain_size()`, `self.contains(idx(i)) ==
    /// expected_fn(i)`.
    #[track_caller]
    fn assert_filled_with(&self, expected_fn: impl Fn(usize) -> bool) {
        for i in 0..self.domain_size() {
            let expected = expected_fn(i);
            assert_eq!(self.contains(idx(i)), expected, "i = {i}");
        }
    }
}

#[test]
fn chunked_bulk_ops() {
    struct ChunkedBulkOp {
        name: &'static str,
        op_fn: fn(&mut ChunkedBitSet<TestIdx>, &ChunkedBitSet<TestIdx>) -> bool,
        spec_fn: fn(fn(usize) -> bool, fn(usize) -> bool, usize) -> bool,
    }
    let ops = &[
        ChunkedBulkOp {
            name: "union",
            op_fn: ChunkedBitSet::union,
            spec_fn: |fizz, buzz, i| fizz(i) || buzz(i),
        },
        ChunkedBulkOp {
            name: "subtract",
            op_fn: ChunkedBitSet::subtract,
            spec_fn: |fizz, buzz, i| fizz(i) && !buzz(i),
        },
        ChunkedBulkOp {
            name: "intersect",
            op_fn: ChunkedBitSet::intersect,
            spec_fn: |fizz, buzz, i| fizz(i) && buzz(i),
        },
    ];

    let domain_sizes = [
        CHUNK_BITS / 7, // Smaller than a full chunk.
        CHUNK_BITS,
        (CHUNK_BITS + CHUNK_BITS / 7), // Larger than a full chunk.
    ];

    for ChunkedBulkOp { name, op_fn, spec_fn } in ops {
        for domain_size in domain_sizes {
            // If false, use different values for LHS and RHS, to test "fizz op buzz".
            // If true, use identical values, to test "fizz op fizz".
            for identical in [false, true] {
                // If false, make a clone of LHS before doing the op.
                // This covers optimizations that depend on whether chunk words are shared or not.
                for unique in [false, true] {
                    // Print the current test case, so that we can see which one failed.
                    println!(
                        "Testing op={name}, domain_size={domain_size}, identical={identical}, unique={unique} ..."
                    );

                    let fizz_fn = |i| i % 3 == 0;
                    let buzz_fn = if identical { fizz_fn } else { |i| i % 5 == 0 };

                    // Check that `fizz op buzz` gives the expected results.
                    chunked_bulk_ops_test_inner(
                        domain_size,
                        unique,
                        fizz_fn,
                        buzz_fn,
                        op_fn,
                        |i| spec_fn(fizz_fn, buzz_fn, i),
                    );
                }
            }
        }
    }
}

fn chunked_bulk_ops_test_inner(
    domain_size: usize,
    unique: bool,
    fizz_fn: impl Fn(usize) -> bool + Copy,
    buzz_fn: impl Fn(usize) -> bool + Copy,
    op_fn: impl Fn(&mut ChunkedBitSet<TestIdx>, &ChunkedBitSet<TestIdx>) -> bool,
    expected_fn: impl Fn(usize) -> bool + Copy,
) {
    // Create two bitsets, "fizz" (LHS) and "buzz" (RHS).
    let mut fizz = ChunkedBitSet::fill_with(domain_size, fizz_fn);
    let buzz = ChunkedBitSet::fill_with(domain_size, buzz_fn);

    // If requested, clone `fizz` so that its word Rcs are not uniquely-owned.
    let _cloned = (!unique).then(|| fizz.clone());

    // Perform the op (e.g. union/subtract/intersect), and verify that the
    // mutated LHS contains exactly the expected values.
    let changed = op_fn(&mut fizz, &buzz);
    fizz.assert_filled_with(expected_fn);

    // Verify that the "changed" return value is correct.
    let should_change = (0..domain_size).any(|i| fizz_fn(i) != expected_fn(i));
    assert_eq!(changed, should_change);
}

fn with_elements_chunked(elements: &[usize], domain_size: usize) -> ChunkedBitSet<TestIdx> {
    let mut s = ChunkedBitSet::new_empty(domain_size);
    for &e in elements {
        assert!(s.insert(idx(e)));
    }
    s
}

#[test]
fn chunked_bitset_iter() {
    fn check_iter(bit: &ChunkedBitSet<TestIdx>, vec: &Vec<usize>) {
        // Test collecting via both `.next()` and `.fold()` calls, to make sure both are correct
        let mut collect_next = Vec::new();
        let mut bit_iter = bit.iter();
        while let Some(item) = bit_iter.next() {
            collect_next.push(item);
        }
        assert_eq!(vec, &collect_next);

        let collect_fold = bit.iter().fold(Vec::new(), |mut v, item| {
            v.push(item);
            v
        });
        assert_eq!(vec, &collect_fold);
    }

    // Empty
    let vec: Vec<usize> = Vec::new();
    let bit = with_elements_chunked(&vec, 9000);
    check_iter(&bit, &vec);

    // Filled
    let n = 10000;
    let vec: Vec<usize> = (0..n).collect();
    let bit = with_elements_chunked(&vec, n);
    check_iter(&bit, &vec);

    // Filled with trailing zeros
    let n = 10000;
    let vec: Vec<usize> = (0..n).collect();
    let bit = with_elements_chunked(&vec, 2 * n);
    check_iter(&bit, &vec);

    // Mixed
    let n = 12345;
    let vec: Vec<usize> = vec![0, 1, 2, 2010, 2047, 2099, 6000, 6002, 6004];
    let bit = with_elements_chunked(&vec, n);
    check_iter(&bit, &vec);
}

#[test]
fn grow() {
    let mut set: GrowableBitSet<TestIdx> = GrowableBitSet::with_capacity(65);
    for index in 0..65 {
        assert!(set.insert(idx(index)));
        assert!(!set.insert(idx(index)));
    }
    set.ensure(128);

    // Check if the bits set before growing are still set
    for index in 0..65 {
        assert!(set.contains(idx(index)));
    }

    // Check if the new bits are all un-set
    for index in 65..128 {
        assert!(!set.contains(idx(index)));
    }

    // Check that we can set all new bits without running out of bounds
    for index in 65..128 {
        assert!(set.insert(idx(index)));
        assert!(!set.insert(idx(index)));
    }
}

#[test]
fn matrix_intersection() {
    let mut matrix: BitMatrix<TestIdx, TestIdx> = BitMatrix::new(200, 200);

    // (*) Elements reachable from both 2 and 65.

    matrix.insert(idx(2), idx(3));
    matrix.insert(idx(2), idx(6));
    matrix.insert(idx(2), idx(10)); // (*)
    matrix.insert(idx(2), idx(64)); // (*)
    matrix.insert(idx(2), idx(65));
    matrix.insert(idx(2), idx(130));
    matrix.insert(idx(2), idx(160)); // (*)

    matrix.insert(idx(64), idx(133));

    matrix.insert(idx(65), idx(2));
    matrix.insert(idx(65), idx(8));
    matrix.insert(idx(65), idx(10)); // (*)
    matrix.insert(idx(65), idx(64)); // (*)
    matrix.insert(idx(65), idx(68));
    matrix.insert(idx(65), idx(133));
    matrix.insert(idx(65), idx(160)); // (*)

    let intersection = matrix.intersect_rows(idx(2), idx(64));
    assert!(intersection.is_empty());

    let intersection = matrix.intersect_rows(idx(2), idx(65));
    assert_eq!(intersection, &[10, 64, 160]);
}

#[test]
fn matrix_iter() {
    let mut matrix: BitMatrix<TestIdx, TestIdx> = BitMatrix::new(64, 100);
    matrix.insert(idx(3), idx(22));
    matrix.insert(idx(3), idx(75));
    matrix.insert(idx(2), idx(99));
    matrix.insert(idx(4), idx(0));
    matrix.union_rows(idx(3), idx(5));
    matrix.insert_all_into_row(idx(6));

    let expected = [99];
    let mut iter = expected.iter();
    for i in matrix.iter(idx(2)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [22, 75];
    let mut iter = expected.iter();
    assert_eq!(matrix.count(idx(3)), expected.len());
    for i in matrix.iter(idx(3)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [0];
    let mut iter = expected.iter();
    assert_eq!(matrix.count(idx(4)), expected.len());
    for i in matrix.iter(idx(4)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [22, 75];
    let mut iter = expected.iter();
    assert_eq!(matrix.count(idx(5)), expected.len());
    for i in matrix.iter(idx(5)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    assert_eq!(matrix.count(idx(6)), 100);
    let mut count = 0;
    for (idx, i) in matrix.iter(idx(6)).enumerate() {
        assert_eq!(idx, i);
        count += 1;
    }
    assert_eq!(count, 100);

    if let Some(i) = matrix.iter(idx(7)).next() {
        panic!("expected no elements in row, but contains element {:?}", i);
    }
}

#[test]
fn sparse_matrix_iter() {
    let mut matrix: SparseBitMatrix<TestIdx, TestIdx> = SparseBitMatrix::new(100);
    matrix.insert(idx(3), idx(22));
    matrix.insert(idx(3), idx(75));
    matrix.insert(idx(2), idx(99));
    matrix.insert(idx(4), idx(0));
    matrix.union_rows(idx(3), idx(5));

    let expected = [99];
    let mut iter = expected.iter();
    for i in matrix.iter(idx(2)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [22, 75];
    let mut iter = expected.iter();
    for i in matrix.iter(idx(3)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [0];
    let mut iter = expected.iter();
    for i in matrix.iter(idx(4)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());

    let expected = [22, 75];
    let mut iter = expected.iter();
    for i in matrix.iter(idx(5)) {
        let j = *iter.next().unwrap();
        assert_eq!(i, j);
    }
    assert!(iter.next().is_none());
}

#[test]
fn sparse_matrix_operations() {
    let mut matrix: SparseBitMatrix<TestIdx, TestIdx> = SparseBitMatrix::new(100);
    matrix.insert(idx(3), idx(22));
    matrix.insert(idx(3), idx(75));
    matrix.insert(idx(2), idx(99));
    matrix.insert(idx(4), idx(0));

    let mut disjoint: DenseBitSet<TestIdx> = DenseBitSet::new_empty(100);
    disjoint.insert(idx(33));

    let mut superset = DenseBitSet::new_empty(100);
    superset.insert(idx(22));
    superset.insert(idx(75));
    superset.insert(idx(33));

    let mut subset = DenseBitSet::new_empty(100);
    subset.insert(idx(22));

    // SparseBitMatrix::remove
    {
        let mut matrix = matrix.clone();
        matrix.remove(idx(3), idx(22));
        assert!(!matrix.row(idx(3)).unwrap().contains(idx(22)));
        matrix.remove(idx(0), idx(0));
        assert!(matrix.row(idx(0)).is_none());
    }

    // SparseBitMatrix::clear
    {
        let mut matrix = matrix.clone();
        matrix.clear(idx(3));
        assert!(!matrix.row(idx(3)).unwrap().contains(idx(75)));
        matrix.clear(idx(0));
        assert!(matrix.row(idx(0)).is_none());
    }

    // SparseBitMatrix::intersect_row
    {
        let mut matrix = matrix.clone();
        assert!(!matrix.intersect_row(idx(3), &superset));
        assert!(matrix.intersect_row(idx(3), &subset));
        matrix.intersect_row(idx(0), &disjoint);
        assert!(matrix.row(idx(0)).is_none());
    }

    // SparseBitMatrix::subtract_row
    {
        let mut matrix = matrix.clone();
        assert!(!matrix.subtract_row(idx(3), &disjoint));
        assert!(matrix.subtract_row(idx(3), &subset));
        assert!(matrix.subtract_row(idx(3), &superset));
        matrix.intersect_row(idx(0), &disjoint);
        assert!(matrix.row(idx(0)).is_none());
    }

    // SparseBitMatrix::union_row
    {
        let mut matrix = matrix.clone();
        assert!(!matrix.union_row(idx(3), &subset));
        assert!(matrix.union_row(idx(3), &disjoint));
        matrix.union_row(idx(0), &disjoint);
        assert!(matrix.row(idx(0)).is_some());
    }
}

#[test]
fn dense_insert_range() {
    #[track_caller]
    fn check<R, I>(domain: usize, range: R, iter: I)
    where
        R: RangeBounds<TestIdx> + Clone + std::fmt::Debug,
        I: Clone + IntoIterator<Item = usize> + std::fmt::Debug,
    {
        let mut set = DenseBitSet::<TestIdx>::new_empty(domain);
        set.insert_range(range.clone());
        for i in set.iter() {
            assert!(range.contains(&i));
        }
        for i in iter {
            assert!(set.contains(idx(i)), "{} in {:?}, inserted {:?}", i, set, range);
        }
    }
    check(300, idx_range(10..10), 10..10);
    check(300, idx_range(WORD_BITS..WORD_BITS * 2), WORD_BITS..WORD_BITS * 2);
    check(300, idx_range(WORD_BITS - 1..WORD_BITS * 2), WORD_BITS - 1..WORD_BITS * 2);
    check(300, idx_range(WORD_BITS - 1..WORD_BITS), WORD_BITS - 1..WORD_BITS);
    check(300, idx_range(10..100), 10..100);
    check(300, idx_range(10..30), 10..30);
    check(300, idx_range(0..5), 0..5);
    check(300, idx_range(0..250), 0..250);
    check(300, idx_range(200..250), 200..250);

    check(300, idx_range_inclusive(10..=10), 10..=10);
    check(300, idx_range_inclusive(WORD_BITS..=WORD_BITS * 2), WORD_BITS..=WORD_BITS * 2);
    check(300, idx_range_inclusive(WORD_BITS - 1..=WORD_BITS * 2), WORD_BITS - 1..=WORD_BITS * 2);
    check(300, idx_range_inclusive(WORD_BITS - 1..=WORD_BITS), WORD_BITS - 1..=WORD_BITS);
    check(300, idx_range_inclusive(10..=100), 10..=100);
    check(300, idx_range_inclusive(10..=30), 10..=30);
    check(300, idx_range_inclusive(0..=5), 0..=5);
    check(300, idx_range_inclusive(0..=250), 0..=250);
    check(300, idx_range_inclusive(200..=250), 200..=250);

    for i in 0..WORD_BITS * 2 {
        for j in i..WORD_BITS * 2 {
            check(WORD_BITS * 2, idx_range(i..j), i..j);
            check(WORD_BITS * 2, idx_range_inclusive(i..=j), i..=j);
            check(300, idx_range(i..j), i..j);
            check(300, idx_range_inclusive(i..=j), i..=j);
        }
    }
}

#[test]
fn dense_last_set_before() {
    fn easy(set: &DenseBitSet<TestIdx>, needle: impl RangeBounds<TestIdx>) -> Option<TestIdx> {
        let mut last_leq = None;
        for e in set.iter() {
            if needle.contains(&e) {
                last_leq = Some(e);
            }
        }
        last_leq
    }

    #[track_caller]
    fn cmp(
        set: &DenseBitSet<TestIdx>,
        needle: impl RangeBounds<TestIdx> + Clone + std::fmt::Debug,
    ) {
        assert_eq!(
            set.last_set_in(needle.clone()),
            easy(set, needle.clone()),
            "{:?} in {:?}",
            needle,
            set
        );
    }
    let mut set = DenseBitSet::<TestIdx>::new_empty(300);
    cmp(&set, idx_range_inclusive(50..=50));
    set.insert(idx(WORD_BITS));
    cmp(&set, idx_range_inclusive(WORD_BITS..=WORD_BITS));
    set.insert(idx(WORD_BITS - 1));
    cmp(&set, idx_range_inclusive(0..=WORD_BITS - 1));
    cmp(&set, idx_range_inclusive(0..=5));
    cmp(&set, idx_range(10..100));
    set.insert(idx(100));
    cmp(&set, idx_range(100..110));
    cmp(&set, idx_range(99..100));
    cmp(&set, idx_range_inclusive(99..=100));

    for i in 0..=WORD_BITS * 2 {
        for j in i..=WORD_BITS * 2 {
            for k in 0..WORD_BITS * 2 {
                let mut set = DenseBitSet::<TestIdx>::new_empty(300);
                cmp(&set, idx_range(i..j));
                cmp(&set, idx_range_inclusive(i..=j));
                set.insert(idx(k));
                cmp(&set, idx_range(i..j));
                cmp(&set, idx_range_inclusive(i..=j));
            }
        }
    }
}

#[test]
fn dense_contains_any() {
    let mut set: DenseBitSet<TestIdx> = DenseBitSet::new_empty(300);
    assert!(!set.contains_any(idx_range(0..300)));
    set.insert_range(idx_range(10..20));
    set.insert_range(idx_range(60..70));
    set.insert_range(idx_range_inclusive(150..=250));

    assert!(set.contains_any(idx_range(0..30)));
    assert!(set.contains_any(idx_range(5..100)));
    assert!(set.contains_any(idx_range(250..255)));

    assert!(!set.contains_any(idx_range(20..59)));
    assert!(!set.contains_any(idx_range(256..290)));

    set.insert(idx(22));
    assert!(set.contains_any(idx_range(20..59)));
}

#[test]
fn borrowed_bit_sets_are_iterable() {
    let mut dense = DenseBitSet::new_empty(8);
    dense.insert(idx(1));
    dense.insert(idx(6));
    assert_eq!((&dense).into_iter().collect::<Vec<_>>(), [idx(1), idx(6)]);

    let mut mixed = MixedBitSet::new_empty(8);
    mixed.insert(idx(2));
    mixed.insert(idx(7));
    assert_eq!((&mixed).into_iter().collect::<Vec<_>>(), [idx(2), idx(7)]);

    let mut growable = GrowableBitSet::new_empty();
    growable.insert(idx(3));
    growable.insert(idx(9));
    assert_eq!((&growable).into_iter().collect::<Vec<_>>(), [idx(3), idx(9)]);
}

#[test]
fn owned_bit_sets_are_iterable() {
    let mut usize_dense = DenseBitSet::new_empty(70);
    usize_dense.insert(1);
    usize_dense.insert(65);
    assert_eq!(usize_dense.into_iter().collect::<Vec<_>>(), [1, 65]);

    let mut dense = DenseBitSet::new_empty(130);
    for index in [0, 63, 64, 129] {
        dense.insert(idx(index));
    }
    assert_eq!(dense.into_iter().collect::<Vec<_>>(), [idx(0), idx(63), idx(64), idx(129)]);

    let mut chunked = ChunkedBitSet::new_empty(5000);
    chunked.insert(idx(3));
    chunked.insert(idx(2050));
    chunked.insert(idx(4099));
    let shared = chunked.clone();
    let expected = [idx(3), idx(2050), idx(4099)];
    assert_eq!((&shared).into_iter().collect::<Vec<_>>(), expected);
    assert_eq!(chunked.into_iter().collect::<Vec<_>>(), expected);
    assert_eq!(
        ChunkedBitSet::<TestIdx>::new_filled(5000).into_iter().collect::<Vec<_>>(),
        (0..5000).map(idx).collect::<Vec<_>>()
    );

    let mut small_mixed = MixedBitSet::new_empty(8);
    small_mixed.insert(idx(2));
    small_mixed.insert(idx(7));
    assert_eq!(small_mixed.into_iter().collect::<Vec<_>>(), [idx(2), idx(7)]);

    let mut large_mixed = MixedBitSet::new_empty(5000);
    large_mixed.insert(idx(4));
    large_mixed.insert(idx(3000));
    assert_eq!(large_mixed.into_iter().collect::<Vec<_>>(), [idx(4), idx(3000)]);

    let mut growable = GrowableBitSet::new_empty();
    growable.insert(idx(3));
    growable.insert(idx(9));
    assert_eq!(growable.into_iter().collect::<Vec<_>>(), [idx(3), idx(9)]);
}
