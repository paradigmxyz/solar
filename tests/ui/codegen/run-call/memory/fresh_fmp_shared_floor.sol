//@ codegen-matrix: standard
//@ run-call: viaFirst => true
//@ run-call: viaSecond 5 => true

// Two entries with different floors share a helper. Its raw read of the free memory pointer and
// the allocation after it must see the same address, so the entries that run it share one
// initial pointer.
contract Test {
    uint256 s0 = 3;
    uint256 s1 = 4;
    uint256 s2 = 5;
    uint256 s3 = 6;
    uint256 s4 = 7;
    uint256 s5 = 8;
    uint256 s6 = 9;
    uint256 s7 = 10;
    uint256 s8 = 11;
    uint256 s9 = 12;
    uint256 s10 = 13;
    uint256 s11 = 14;
    uint256 s12 = 15;
    uint256 s13 = 16;
    uint256 s14 = 17;
    uint256 s15 = 18;
    uint256 s16 = 19;
    uint256 s17 = 20;
    uint256 sink;

    function same() internal returns (bool) {
        uint256 ptr;
        assembly ("memory-safe") {
            ptr := mload(0x40)
        }
        bytes memory b = new bytes(32);
        uint256 object;
        assembly ("memory-safe") {
            object := b
        }
        sink = s0 + s1 + s2 + s3 + s4;
        return ptr == object;
    }

    function viaFirst() external returns (bool) {
        return same();
    }

    function viaSecond(uint256 n) external returns (bool ok) {
        uint256 t0 = s0 * 2 + n;
        uint256 t1 = s1 * 3 + n;
        uint256 t2 = s2 * 4 + n;
        uint256 t3 = s3 * 5 + n;
        uint256 t4 = s4 * 6 + n;
        uint256 t5 = s5 * 7 + n;
        uint256 t6 = s6 * 8 + n;
        uint256 t7 = s7 * 9 + n;
        uint256 t8 = s8 * 10 + n;
        uint256 t9 = s9 * 11 + n;
        uint256 t10 = s10 * 12 + n;
        uint256 t11 = s11 * 13 + n;
        uint256 t12 = s12 * 14 + n;
        uint256 t13 = s13 * 15 + n;
        uint256 t14 = s14 * 16 + n;
        uint256 t15 = s15 * 17 + n;
        uint256 t16 = s16 * 18 + n;
        uint256 t17 = s17 * 19 + n;
        ok = same();
        unchecked {
            sink = t0 * 1 + t1 * 2 + t2 * 3 + t3 * 4 + t4 * 5 + t5 * 6 + t6 * 7 + t7 * 8 + t8 * 9 + t9 * 10 + t10 * 11 + t11 * 12 + t12 * 13 + t13 * 14 + t14 * 15 + t15 * 16 + t16 * 17 + t17 * 18;
        }
    }
}
