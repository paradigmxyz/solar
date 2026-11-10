//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: fill 3 => 3, [1, 2, 3], 0x0300000000000000000000000000000300000000000000020000000000000001, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: fill 4 => 4, [1, 2, 3, 4], 0x0000000000000000000000000000000000000000000000000000000000000004, 0x0000000000000004000000000000000300000000000000020000000000000001
//@ run-call: fill 6 => 6, [1, 2, 3, 4, 5, 6], 0x0000000000000000000000000000000000000000000000000000000000000006, 0x0000000000000004000000000000000300000000000000020000000000000001
//@ run-call: fill 0 => 0, [], 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: writes 2 => [1, 9]
//@ run-call: writes 5 => [1, 9, 3, 4, 5]
//@ run-call: pops 3 => [1], 0x0100000000000000000000000000000000000000000000000000000000000001
//@ run-call: pops 6 => [1, 2, 3, 4], 0x0000000000000000000000000000000000000000000000000000000000000004
//@ run-call: refill => [7], 0x0100000000000000000000000000000000000000000000000000000000000007
//@ run-call-fail: popEmpty() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000031
//@ run-call-fail: outOfBounds 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: outOfBounds 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: deletes 2 => 0, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: deletes 5 => 0, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: getters 2 => 20
//@ run-call-fail: getterOutOfBounds() => 0x
//@ run-call: pushZero => [0, 7], 2
//@ run-call: triples 10 => 10, 0x0a00a00009a00008a00007a00006a00005a00004a00003a00002a00001a00000, 0x0000000000000000000000000000000000000000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: triples 11 => 11, 0x000000000000000000000000000000000000000000000000000000000000000b, 0x0000a00009a00008a00007a00006a00005a00004a00003a00002a00001a00000, 0x0000000000000000000000000000000000000000000000000000000000a0000a
//@ run-call: signed => [-1, 2, -32768], 0x030000000000000000000000000000000000000000000000000080000002ffff
//@ run-call: copies 2 => [1, 2], true
//@ run-call: copies 5 => [1, 2, 3, 4, 5], true
//@ run-call: enumCopies => [1], [2]

// A storage array documented `@custom:solar-storage inline` keeps its length in the
// top byte of its slot and its elements below it while they fit; past that it
// moves to the standard layout. Reads, writes, pushes, pops, deletes, copies
// and getters behave as without the tag; raw reads of the slot and the first
// data word show the layout.
contract InlineArrays {
    // CHECK-LABEL: fn @list(
    // CHECK: [[HEADER:v[0-9]+]] = sload 0
    // CHECK: [[TOP:v[0-9]+]] = shr [[HEADER]], 248
    // CHECK: [[INLINE:v[0-9]+]] = ne [[TOP]], 0
    // CHECK: [[LENGTH:v[0-9]+]] = select [[INLINE]], [[TOP]], [[HEADER]]
    // CHECK: [[VALID:v[0-9]+]] = lt arg0, [[LENGTH]]
    // CHECK: icall revert_if_zero<empty>, [[VALID]]
    /// @custom:solar-storage inline
    uint64[] public list;
    /// @custom:solar-storage inline
    bytes3[] codes;
    /// @custom:solar-storage inline
    int16[] deltas;

    function raw() internal view returns (bytes32 head, bytes32 data0, bytes32 data1) {
        bytes32 data = keccak256(abi.encode(uint256(0)));
        assembly {
            head := sload(0)
            data0 := sload(data)
            data1 := sload(add(data, 1))
        }
    }

    // CHECK-LABEL: fn @fill(
    // CHECK: icall @load_storage_inline_array_8_0, 0
    // CHECK: icall @inline_array_push_8_0, 0,
    function fill(uint256 n) external returns (uint256, uint64[] memory, bytes32 head, bytes32 data0) {
        for (uint256 i; i < n; ++i) list.push(uint64(i + 1));
        (head, data0, ) = raw();
        return (list.length, list, head, data0);
    }

    function writes(uint256 n) external returns (uint64[] memory) {
        for (uint256 i; i < n + 1; ++i) list.push(uint64(i + 1));
        list.pop();
        list[1] = 9;
        return list;
    }

    // Popping clears the element; a spilled array stays standard until it is empty.
    // CHECK-LABEL: fn @pops(
    // CHECK: icall @inline_array_pop_8_0, 0
    // CHECK-NEXT: icall @inline_array_pop_8_0, 0
    // CHECK: icall @load_storage_inline_array_8_0, 0
    function pops(uint256 n) external returns (uint64[] memory, bytes32 head) {
        for (uint256 i; i < n; ++i) list.push(uint64(i + 1));
        list.pop();
        list.pop();
        (head, , ) = raw();
        return (list, head);
    }

    // An array emptied by pops takes the inline form again.
    function refill() external returns (uint64[] memory, bytes32 head) {
        for (uint256 i; i < 5; ++i) list.push(uint64(i + 1));
        for (uint256 i; i < 5; ++i) list.pop();
        list.push(7);
        (head, , ) = raw();
        return (list, head);
    }

    function popEmpty() external {
        list.pop();
    }

    // CHECK-LABEL: fn @outOfBounds(
    // CHECK: shr {{v[0-9]+}}, 248
    // CHECK: lt {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK: icall panic_if<0x32>,
    function outOfBounds(uint256 n) external returns (uint64) {
        for (uint256 i; i < n; ++i) list.push(uint64(i + 1));
        return list[n];
    }

    // CHECK-LABEL: fn @deletes(
    // CHECK: sstore 0, 0
    // CHECK: clear_storage_words 0, 0,
    // CHECK: sstore 0, 0
    function deletes(uint256 n) external returns (uint256, bytes32 data0) {
        for (uint256 i; i < n; ++i) list.push(uint64(i + 1));
        delete list;
        (, data0, ) = raw();
        return (list.length, data0);
    }

    function getters(uint256 n) external returns (uint64) {
        for (uint256 i; i < n; ++i) list.push(uint64((i + 1) * 10));
        return this.list(1);
    }

    function getterOutOfBounds() external returns (uint64) {
        list.push(1);
        return this.list(1);
    }

    function pushZero() external returns (uint64[] memory, uint256) {
        list.push();
        list.push() = 7;
        return (list, list.length);
    }

    // Ten 3-byte elements fill the slot below the length byte, and fill the first
    // data word too, so the eleventh starts the second.
    function triples(uint256 n)
        external
        returns (uint256, bytes32 head, bytes32 data0, bytes32 data1)
    {
        for (uint256 i; i < n; ++i) codes.push(bytes3(uint24(0xa00000 + i)));
        bytes32 data = keccak256(abi.encode(uint256(1)));
        assembly {
            head := sload(1)
            data0 := sload(data)
            data1 := sload(add(data, 1))
        }
        for (uint256 i; i < n; ++i) require(codes[i] == bytes3(uint24(0xa00000 + i)));
        return (codes.length, head, data0, data1);
    }

    uint64[] plain;

    // A storage copy into an untagged array, and an encoding, read the tagged one as it is.
    // CHECK-LABEL: fn @copies(
    // CHECK: [[COPY:v[0-9]+]] = icall @load_storage_inline_array_8_0, 0
    // CHECK: [[COPY_LEN:v[0-9]+]] = mload [[COPY]]
    // CHECK: sstore 3, [[COPY_LEN]]
    // CHECK: icall @load_storage_packed_array_8_0, 3
    // CHECK: [[ENCODE_COPY:v[0-9]+]] = icall @load_storage_inline_array_8_0, 0
    // CHECK: abi_encode [memory_array<word<uint64>>], scratch, args [[ENCODE_COPY]]
    // CHECK: [[COPY_HASH:v[0-9]+]] = keccak256
    // CHECK: abi_encode [memory_array<word<uint64>>], scratch, args
    // CHECK: [[EXPECTED_HASH:v[0-9]+]] = keccak256
    // CHECK: eq [[COPY_HASH]], [[EXPECTED_HASH]]
    function copies(uint256 n) external returns (uint64[] memory, bool) {
        uint64[] memory expected = new uint64[](n);
        for (uint256 i; i < n; ++i) {
            list.push(uint64(i + 1));
            expected[i] = uint64(i + 1);
        }
        plain = list;
        return (plain, keccak256(abi.encode(list)) == keccak256(abi.encode(expected)));
    }

    // CHECK-LABEL: fn @signed(
    // CHECK: icall @inline_array_push_2_1, 2, 0xffff
    // CHECK: icall @inline_array_push_2_1, 2, 2
    // CHECK: icall @inline_array_push_2_1, 2, 0x8000
    // CHECK: icall @load_storage_inline_array_2_1, 2
    function signed() external returns (int16[] memory, bytes32 head) {
        deltas.push(-1);
        deltas.push(2);
        deltas.push(type(int16).min);
        assembly {
            head := sload(2)
        }
        return (deltas, head);
    }

    enum Small { Zero, One }
    enum Large { Zero, One, Two }

    /// @custom:solar-storage inline
    Small[] small;
    /// @custom:solar-storage inline
    Large[] large;

    // Copy helpers must keep each enum's range when their elements occupy the same byte.
    // CHECK-LABEL: fn @enumCopies(
    // CHECK: icall @inline_array_push_1_0_enum_2, 4, 1
    // CHECK: icall @inline_array_push_1_0_enum_3, 5, 2
    // CHECK: icall @load_storage_inline_array_1_0_enum_2, 4
    // CHECK: icall @load_storage_inline_array_1_0_enum_3, 5
    function enumCopies() external returns (Small[] memory, Large[] memory) {
        small.push(Small.One);
        large.push(Large.Two);
        Small[] memory first = small;
        Large[] memory second = large;
        return (first, second);
    }

    // CHECK-LABEL: fn @inline_array_push_8_0(
    // CHECK: shr {{v[0-9]+}}, 248
    // CHECK: lt {{v[0-9]+}}, 3
    // CHECK: storage_array_data_slot arg0
    // CHECK: sstore arg0, 4
    // CHECK: add {{v[0-9]+}}, 0x100000000000000000000000000000000000000000000000000000000000000

    // CHECK-LABEL: fn @inline_array_pop_8_0(
    // CHECK: icall panic_if<0x31>,
    // CHECK: [[LAST:v[0-9]+]] = sub {{v[0-9]+}}, 1
    // CHECK: [[BITS:v[0-9]+]] = mul [[LAST]], 64
    // CHECK: [[END:v[0-9]+]] = shl 1, [[BITS]]
    // CHECK: [[MASK:v[0-9]+]] = sub [[END]], 1
    // CHECK: [[ELEMENTS:v[0-9]+]] = and {{v[0-9]+}}, [[MASK]]
    // CHECK: [[LENGTH:v[0-9]+]] = shl [[LAST]], 248
    // CHECK: [[SHRUNK:v[0-9]+]] = or [[ELEMENTS]], [[LENGTH]]
    // CHECK: sstore arg0, [[SHRUNK]]

    // CHECK-LABEL: fn @load_storage_inline_array_1_0_enum_2(
    // CHECK: lt {{v[0-9]+}}, i8 2
    // CHECK: icall panic_if<0x21>,

    // CHECK-LABEL: fn @load_storage_inline_array_1_0_enum_3(
    // CHECK: lt {{v[0-9]+}}, i8 3
    // CHECK: icall panic_if<0x21>,
}
