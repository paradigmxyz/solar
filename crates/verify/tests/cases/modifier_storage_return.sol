pragma solidity ^0.8.20;
// A function's storage return keeps what its body bound it to across its modifier's `_`.
contract ModifierStorageReturn {
    uint256[1] internal a;
    uint256[1] internal b;

    modifier after_() {
        _;
        a[0] = 7;
    }

    function _pick() private after_ returns (uint256[1] storage r) {
        r = a;
    }

    /// @custom:ensures result == 7
    function ok_storage_return() external returns (uint256 result) {
        a[0] = 1;
        b[0] = 1;
        uint256[1] storage p = _pick();
        return p[0];
    }

    // The modifier writes 7 to `a[0]` before `p` is read.
    /// @custom:ensures result == 1
    function bad_storage_return() external returns (uint256 result) {
        a[0] = 1;
        b[0] = 1;
        uint256[1] storage p = _pick();
        return p[0];
    }
}
