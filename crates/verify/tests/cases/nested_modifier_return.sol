pragma solidity ^0.8.20;
// Two modifiers around a function with a storage return: the outer one's `_` keeps what the
// body bound the return to.
contract NestedModifierReturn {
    uint256[1] internal a;

    modifier outer() {
        _;
        a[0] = 7;
    }

    modifier inner() {
        _;
    }

    function _pick() private outer inner returns (uint256[1] storage r) {
        r = a;
    }

    /// @custom:ensures result == 7
    function ok_nested() external returns (uint256 result) {
        a[0] = 1;
        uint256[1] storage p = _pick();
        return p[0];
    }

    // The outer modifier writes 7 to `a[0]` before `p` is read.
    /// @custom:ensures result == 1
    function bad_nested() external returns (uint256 result) {
        a[0] = 1;
        uint256[1] storage p = _pick();
        return p[0];
    }
}
