//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: pack 0x414243444546 => 0x4142430044454600
//@ run-call: pack 0x => 0x
//@ run-call: rawBase 0 => 7
//@ run-call-fail: rawBase 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0

// A loop that steps one counter over its input and another over its output
// reduces both address families. The output family's three offsets share one
// carried pointer, so the loop adds to the pointer rather than rebuilding each
// address from the counter, and the write offsets are small constants.
// CHECK-LABEL: fn @pack
// CHECK: mstore8 [[OUT:v[0-9]+]],
// CHECK: [[P1:v[0-9]+]] = add [[OUT]], 1
// CHECK: mstore8 [[P1]],
// CHECK: [[P2:v[0-9]+]] = add [[OUT]], 2
// CHECK: mstore8 [[P2]],
// CHECK: [[P3:v[0-9]+]] = add [[OUT]], 3
// CHECK: mstore8 [[P3]],
// CHECK: add [[OUT]], 4

contract Test {
    // Copies three input bytes into every four output bytes, so the read and
    // the write counters advance by different amounts.
    function pack(bytes memory s) public pure returns (bytes memory out) {
        uint256 n = s.length;
        uint256 length = (n / 3) * 4;
        out = new bytes(length);
        uint256 i;
        uint256 j;
        while (i + 2 < n && j + 3 < length) {
            out[j] = s[i];
            out[j + 1] = s[i + 1];
            out[j + 2] = s[i + 2];
            out[j + 3] = 0;
            i += 3;
            j += 4;
        }
    }

    function rawBase(uint256 base) external pure returns (uint256 result) {
        assembly {
            for { let i := 0 } lt(i, 2) { i := add(i, 1) } {
                mstore(add(base, shl(5, i)), 7)
            }
            result := mload(0)
        }
    }
}
