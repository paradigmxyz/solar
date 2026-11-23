pragma solidity ^0.8.20;
// A storage pointer declared in a block is out of scope after it: a later memory local of the
// same name is its own object, which a callee writing its memory parameter changes.
contract StoragePointerScope {
    uint256[1] internal st;

    function fill(uint256[1] memory p) private pure {
        p[0] = 7;
    }

    // Solidity returns 7.
    /// @custom:ensures result == 0
    function bad_expired_pointer() external returns (uint256 result) {
        {
            uint256[1] storage tmp = st;
            tmp[0] = 0;
        }
        {
            uint256[1] memory tmp;
            fill(tmp);
            return tmp[0];
        }
    }
}
