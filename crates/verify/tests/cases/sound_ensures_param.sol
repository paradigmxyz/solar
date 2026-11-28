pragma solidity ^0.8.20;
// A postcondition reads a value parameter as the caller passed it, which is what callers assume
// of their arguments: a body that reassigns the parameter does not change what the
// postcondition says.
contract P {
    uint256 x;

    // x ends as a + 1 for the caller's a.
    /// @custom:ensures x == a
    function bad_h(uint256 a) internal {
        a = a + 1;
        x = a;
    }

    /// @custom:ensures x == a + 1
    function h2(uint256 a) internal {
        a = a + 1;
        x = a;
    }

    /// @custom:ensures x == 6
    function ok_caller() public {
        h2(5);
    }

    /// @custom:ensures x == 5
    function bad_caller() public {
        h2(5);
    }

    // k(3) returns 6.
    /// @custom:ensures result == a
    function bad_k(uint256 a) public pure returns (uint256) {
        a = a * 2;
        return a;
    }

    /// @custom:ensures result == 2 * a
    function ok_k(uint256 a) public pure returns (uint256) {
        a = a * 2;
        return a;
    }
}
