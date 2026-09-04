//@ codegen-matrix: standard
//@ run-call: LibraryStoragePointerMultiReturn::forwardPair => 2, [11, 22]
//@ run-call: LibraryStoragePointerMultiReturn::forwardBytesPair => 3, 0xaabbcc
//@ run-call: LibraryStoragePointerMultiReturn::forwardStructPair => 7, (7, [33])
//@ run-call: LibraryStoragePointerMultiReturn::forwardNarrowPair => 2, [11, 22]
//@ run-call: LibraryStoragePointerMultiReturn::forwardLeadingPair => [11, 22], 2
//@ run-call: LibraryStoragePointerMultiReturn::forwardTriple => [11, 22], 2, 0xaabbcc
//@ run-call: LibraryStoragePointerMultiReturn::declarePair => 2, [11, 22]
//@ run-call: LibraryStoragePointerMultiReturn::assignPair => 2, [11, 22]
//@ run-call: LibraryStoragePointerMultiReturn::forwardKeepingStorage => 2, 2

// A library function returning a storage reference hands its caller the slot word, so a caller
// declared to return the same component in memory must copy the object out of storage instead of
// forwarding the slot as if it were a memory pointer. Expected values are solc 0.8.36's.
library Lib {
    struct S {
        uint256 a;
        uint256[] v;
    }

    function pair(uint256[] storage a) internal view returns (uint256, uint256[] storage) {
        return (a.length, a);
    }

    function bytesPair(bytes storage b) internal view returns (uint256, bytes storage) {
        return (b.length, b);
    }

    function structPair(S storage s) internal view returns (uint256, S storage) {
        return (s.a, s);
    }

    function narrowPair(uint256[] storage a) internal view returns (uint8, uint256[] storage) {
        return (uint8(a.length), a);
    }

    function leadingPair(uint256[] storage a)
        internal
        view
        returns (uint256[] storage, uint256)
    {
        return (a, a.length);
    }

    function triple(uint256[] storage a, bytes storage b)
        internal
        view
        returns (uint256[] storage, uint256, bytes storage)
    {
        return (a, a.length, b);
    }
}

contract LibraryStoragePointerMultiReturn {
    uint256[] private nums;
    bytes private bs;
    Lib.S private s;

    constructor() {
        nums.push(11);
        nums.push(22);
        bs = hex"aabbcc";
        s.a = 7;
        s.v.push(33);
    }

    function forwardPair() external view returns (uint256, uint256[] memory) {
        return Lib.pair(nums);
    }

    function forwardBytesPair() external view returns (uint256, bytes memory) {
        return Lib.bytesPair(bs);
    }

    function forwardStructPair() external view returns (uint256, Lib.S memory) {
        return Lib.structPair(s);
    }

    // The value component also needs its own conversion, here a widening one.
    function forwardNarrowPair() external view returns (uint256, uint256[] memory) {
        return Lib.narrowPair(nums);
    }

    function forwardLeadingPair() external view returns (uint256[] memory, uint256) {
        return Lib.leadingPair(nums);
    }

    function forwardTriple() external view returns (uint256[] memory, uint256, bytes memory) {
        return Lib.triple(nums, bs);
    }

    function declarePair() external view returns (uint256, uint256[] memory) {
        (uint256 n, uint256[] memory m) = Lib.pair(nums);
        return (n, m);
    }

    function assignPair() external view returns (uint256, uint256[] memory) {
        uint256 n;
        uint256[] memory m;
        (n, m) = Lib.pair(nums);
        return (n, m);
    }

    // A declared storage return keeps the slot, which is what the callee already returned.
    function keepStorage() internal view returns (uint256, uint256[] storage) {
        return Lib.pair(nums);
    }

    function forwardKeepingStorage() external view returns (uint256, uint256) {
        (uint256 n, uint256[] storage r) = keepStorage();
        return (n, r.length);
    }
}
