//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: uncheckedDown 0x0102030405, 1, 3 => 0x020304
//@ run-call: uncheckedDown 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0x
//@ run-call-fail: uncheckedDown 0x0102, 1, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: uncheckedDown 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: uncheckedDown 0x0102, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: checkedDown 0x0102030405, 1, 3 => 0x020304
//@ run-call-fail: checkedDown 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: mixedDown 0x0102030405, 1, 3 => 0x020304
//@ run-call-fail: mixedDown 0x01, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: mixedDown 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: sourceFirst 0x0102030405, 1, 0, 3 => 0x020304
//@ run-call-fail: sourceFirst 0x0102, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: destinationFirst 0x01, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: wrappedCopy 0x, 4 => 0x00bd307f
//@ run-call: wrappedWord 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x00c280cc68000000000000000000000000000000000000000000000000000000
//@ run-call-fail: wrappedArray [], 0, 0x0800000000000000000000000000000000000000000000000000000000000000, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: wrappedArray [], 32, 0x07ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: wrappedArray [], 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: wrappedArray [], 0, 0x07ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => [0]
//@ run-call: wrappedArray [], 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => []
//@ run-call: wrappedArray [], 0, 6, 1 => [0]

// A downward copy's first iteration indexes `offset + count - 1`. A checked addition that wraps
// panics there with `Panic(0x11)`. An unchecked one wraps silently, and the falling index then
// reaches `2^256 - 1` in some iteration, where its bounds check raises `Panic(0x32)`.
contract Test {
    // Unchecked index arithmetic fails with `Panic(0x32)` when it wraps.
    // CHECK-LABEL: fn @uncheckedDown(
    // CHECK-NOT: mstore8
    // CHECK: [[END:v[0-9]+]] = add arg1, arg2
    // CHECK-NEXT: [[WRAPS:v[0-9]+]] = lt [[END]], arg1
    // CHECK-NEXT: [[PAST:v[0-9]+]] = gt [[END]], {{v[0-9]+}}
    // CHECK-NEXT: [[FAILS:v[0-9]+]] = or [[WRAPS]], [[PAST]]
    // CHECK-NEXT: jumpi [[FAILS]]
    // CHECK: calldatacopy
    // CHECK-NOT: mstore 32, 17
    function uncheckedDown(bytes calldata src, uint256 from, uint256 count)
        external
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        unchecked {
            for (uint256 k = count; k > 0; --k) {
                out[k - 1] = src[from + k - 1];
            }
        }
    }

    // Checked arithmetic fails with `Panic(0x11)`.
    // CHECK-LABEL: fn @checkedDown(
    // CHECK-NOT: mstore8
    // CHECK: [[END:v[0-9]+]] = add arg1, arg2
    // CHECK-NEXT: [[WRAPS:v[0-9]+]] = lt [[END]], arg1
    // CHECK-NEXT: jumpi [[WRAPS]]
    // CHECK: calldatacopy
    // CHECK: mstore 32, 17
    function checkedDown(bytes calldata src, uint256 from, uint256 count)
        external
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        for (uint256 k = count; k > 0; --k) {
            out[k - 1] = src[from + k - 1];
        }
    }

    // An unchecked sum whose checked decrement fails where the sum reaches zero stays a loop.
    // CHECK-LABEL: fn @mixedDown(
    // CHECK: mstore8
    function mixedDown(bytes calldata src, uint256 from, uint256 count)
        external
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        for (uint256 k = count; k > 0; --k) {
            uint256 end;
            unchecked {
                end = from + k;
            }
            out[k - 1] = src[end - 1];
        }
    }

    // So does a wrapping source index ahead of a checked destination addition, which fails first.
    // CHECK-LABEL: fn @sourceFirst(
    // CHECK: mstore8
    function sourceFirst(bytes calldata src, uint256 from, uint256 to, uint256 count)
        external
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        for (uint256 k = count; k > 0; --k) {
            uint256 at;
            unchecked {
                at = from + k - 1;
            }
            out[to + k - 1] = src[at];
        }
    }

    // A destination index computed in an earlier statement fails before the source's check, so the
    // loop stays.
    // CHECK-LABEL: fn @destinationFirst(
    // CHECK: mstore8
    function destinationFirst(bytes calldata src, uint256 from, uint256 to, uint256 count)
        external
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        for (uint256 k = count; k > 0; --k) {
            uint256 at = to + k - 1;
            out[at] = src[from + k - 1];
        }
    }

    // A slice offset inline assembly set near `2^256` wraps: the loop reads the start of calldata
    // past the wrap, where `calldatacopy` and `calldataload` read zeros, so both keep their loops
    // behind a run-time test.
    // CHECK-LABEL: fn @wrappedCopy(
    // CHECK: calldatacopy
    // CHECK: mstore8
    function wrappedCopy(bytes calldata data, uint256 n) external pure returns (bytes memory out) {
        assembly {
            data.offset := not(0)
            data.length := n
        }
        out = new bytes(n);
        for (uint256 k; k < n; ++k) {
            out[k] = data[k];
        }
    }

    // CHECK-LABEL: fn @wrappedWord(
    // CHECK: calldataload
    // CHECK: byte 0
    function wrappedWord(bytes calldata data) external pure returns (bytes32 result) {
        assembly {
            data.offset := not(0)
        }
        for (uint256 k; k < 32; ++k) {
            result |= bytes32(data[k]) >> (8 * k);
        }
    }

    // Scaling an index to bytes must preserve the loop's 0x41 check even when assembly forges
    // a slice length large enough to admit that index. Crossing the index limit can also lose
    // bits when the resulting byte address itself does not wrap.
    // CHECK-LABEL: fn @wrappedArray(
    // CHECK: [[INDEX_END:v[0-9]+]] = add arg2, arg3
    // CHECK-NEXT: [[SCALE_WRAPS:v[0-9]+]] = shr [[INDEX_END]], 251
    // CHECK-NEXT: [[INDEX_WRAPS:v[0-9]+]] = lt [[INDEX_END]], arg2
    // CHECK-NEXT: [[WRAPS_WORD:v[0-9]+]] = zext i1 [[INDEX_WRAPS]] to i256
    // CHECK-NEXT: [[INDEX_FAILS:v[0-9]+]] = or [[WRAPS_WORD]], [[SCALE_WRAPS]]
    // CHECK-NEXT: [[INDEX_FAILS_WORD:v[0-9]+]] = ne [[INDEX_FAILS]], 0
    // CHECK-NEXT: [[FALLBACK:v[0-9]+]] = or {{v[0-9]+}}, [[INDEX_FAILS_WORD]]
    // CHECK-NEXT: jumpi [[FALLBACK]]
    // CHECK: calldatacopy
    // CHECK: phi
    // CHECK: calldataload
    function wrappedArray(uint256[] calldata source, uint256 at, uint256 offset, uint256 count)
        external pure returns (uint256[] memory out)
    {
        assembly {
            source.offset := at
            source.length := not(0)
        }
        out = new uint256[](count);
        for (uint256 k; k < count; ++k) {
            out[k] = source[offset + k];
        }
    }
}
