//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// `@custom:solar-inline` asks for every legal clone, so a tagged helper inlines even where many
// caller words survive the call or the caller has grown: the stack and growth budgets that keep
// untagged helpers shared do not apply. `byte-loops` then turns the helper's loop into one load.
library L {
    /// @custom:solar-inline
    function _readBytes32(bytes memory b, uint256 offset) internal pure returns (bytes32 r) {
        for (uint256 k; k < 32; ++k) {
            r |= bytes32(b[offset + k]) >> (k << 3);
        }
    }

    /// @custom:solar-inline
    function _writeBytes32(bytes memory b, uint256 offset, bytes32 value) internal pure {
        for (uint256 k; k < 32; ++k) {
            b[offset + k] = value[k];
        }
    }

    // CHECK-LABEL: fn @f{{.*}}(arg0: i256
    // CHECK-NOT: icall @_readBytes32
    // CHECK: ret
    function f(bytes memory x, bytes memory out, uint256 a, uint256 b2, uint256 c, uint256 d)
        internal
        pure
        returns (uint256 o)
    {
        for (uint256 i; i < a; ++i) {
            uint256 from = o - (i & 7);
            for (uint256 j; j < b2; j += 32) {
                _writeBytes32(out, o + j, _readBytes32(out, from + j));
            }
            o += c ^ d ^ uint256(_readBytes32(x, i));
        }
    }
}

contract TaggedBudgets {
    function run(bytes memory x, bytes memory out, uint256 a, uint256 b, uint256 c, uint256 d)
        external
        pure
        returns (uint256)
    {
        return L.f(x, out, a, b, c, d);
    }
}
