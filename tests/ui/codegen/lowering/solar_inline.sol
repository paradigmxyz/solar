//@ compile-flags: -Onone -Zdump=mir
//@ filecheck:
//@ run-call: find 0x0a0b0c0d0e0f, 0x0d => 3
//@ run-call: find 0x0aff0c, 0x07 => 1

// `@custom:solar-inline` marks the function `always_inline` for the inliner.
// CHECK-LABEL: fn @_at(
// CHECK-SAME: always_inline

contract Test {
    function find(bytes memory s, bytes1 c) public pure returns (uint256) {
        for (uint256 i; i < s.length; ++i) {
            if (_at(s, i) == c) return i;
        }
        return type(uint256).max;
    }

    /// @custom:solar-inline
    function _at(bytes memory s, uint256 i) private pure returns (bytes1) {
        bytes1 b = s[i];
        if (b == 0xff) return s[s.length - 1] ^ s[i / 2] ^ bytes1(uint8(i));
        return b;
    }
}
