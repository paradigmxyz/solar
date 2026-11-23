pragma solidity ^0.8.20;
// A memory local declared in a for loop's init is in scope for the loop only: an outer storage
// pointer of its name still writes storage after the loop.
contract ForInitShadow {
    uint256[1] internal st;

    /// @custom:ensures st[0] == 5
    function ok_for_init_shadow() external {
        uint256[1] storage p = st;
        p[0] = 0;
        for (uint256[1] memory p; p[0] > 0; ) {}
        p[0] = 5;
    }

    // Solidity writes 5 to `st[0]` through the outer `p`.
    /// @custom:ensures st[0] == 0
    function bad_for_init_shadow() external {
        uint256[1] storage p = st;
        p[0] = 0;
        for (uint256[1] memory p; p[0] > 0; ) {}
        p[0] = 5;
    }
}
