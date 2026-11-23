pragma solidity ^0.8.20;
// After `_` a modifier's own names are its own, even when the function's storage return
// shares one of them.
contract ModifierReturnName {
    uint256[1] internal a;
    uint256 internal seen;

    modifier note(uint256 r) {
        _;
        seen = r;
    }

    function _pick(uint256 v) private note(v) returns (uint256[1] storage r) {
        r = a;
    }

    /// @custom:ensures seen == v
    function ok_modifier_param(uint256 v) external {
        _pick(v);
    }

    /// @custom:ensures seen == 0
    function bad_modifier_param(uint256 v) external {
        _pick(v);
    }
}
