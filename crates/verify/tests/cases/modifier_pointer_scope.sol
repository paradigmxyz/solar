pragma solidity ^0.8.20;
// A modifier's storage pointer stays bound across its `_`: a local of the same name that the
// function body declares is the body's own.
contract ModifierPointerScope {
    uint256[1] internal a;
    uint256[1] internal p;

    modifier m() {
        a[0] = 0;
        uint256[1] storage p = a;
        {
            _;
        }
        p[0] = 7;
    }

    /// @custom:ensures a[0] == 7
    function ok_modifier_pointer() external m {
        uint256[1] memory p;
        p;
    }

    // The modifier writes 7 to `a[0]` after the body.
    /// @custom:ensures a[0] == 0
    function bad_modifier_pointer() external m {
        uint256[1] memory p;
        p;
    }
}
