//@ compile-flags: -Ogas -Zdump=mir-final
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The byte offset of a calldata element wraps modulo `2^256`, as in solc, so indexing a calldata
// array with an index the optimizer cannot bound adds no overflow test and no `Panic(0x41)` block.
contract CalldataIndexOffset {
    // CHECK-LABEL: fn @sum(
    // CHECK-NOT: shr {{.*}}, 251
    // CHECK-NOT: mstore 32, 65
    function sum(uint256[] calldata a, uint256[] calldata b) internal pure returns (uint256 s) {
        for (uint256 i; i < a.length; ++i) {
            s += a[i] * b[a.length - i - 1];
        }
    }

    // CHECK-LABEL: fn @f(
    function f(uint256[] calldata a, uint256[] calldata b) external pure returns (uint256) {
        return sum(a, b);
    }

    function g(uint256[] calldata a, uint256[] calldata b) external pure returns (uint256) {
        return sum(b, a) + 1;
    }
}
