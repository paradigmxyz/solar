pragma solidity ^0.8.20;
// A return variable has no value before the call: old() of one is an error, not an unknown.
contract OldReturn {
    uint256 x;

    /// @custom:ensures r == old(r) + 1
    // expect: of return variable `r`, which has no value before the call
    function bad_old_ret() external view returns (uint256 r) {
        r = x + 1;
    }

    /// @custom:ensures r == old(x) + 1
    function ok_old_state() external view returns (uint256 r) {
        r = x + 1;
    }
}
