pragma solidity ^0.8.20;
// A storage pointer declared in a for loop's init is in scope for the loop only: the outer local
// of its name is seen again after the loop, not a contract constant of that name.
contract ForInitPointer {
    uint256 internal constant p = 9;
    uint256[1] internal st;

    // Solidity returns the outer local, 3.
    /// @custom:ensures result == 9
    function bad_for_init_pointer() external returns (uint256 result) {
        {
            uint256 p = 3;
            for (uint256[1] storage p = st; p[0] > 0; ) {
                p[0] = 0;
            }
            return p;
        }
    }
}
