pragma solidity ^0.8.20;
// A bound library call runs the overload the receiver's type picks: every candidate's writes
// count before an earlier `abi.encode` argument is hashed from its words.
library Steps {
    function step(bool, uint256[1] memory) internal pure returns (uint256) {
        return 2;
    }

    function step(uint256, uint256[1] memory a) internal pure returns (uint256) {
        a[0] = 1;
        return 2;
    }
}

contract EncodeUsingOverload {
    using Steps for bool;
    using Steps for uint256;

    // The `uint256` overload writes `a[0]` before the compiled code encodes `a`.
    // expect: postcondition not satisfied
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_using_overload(uint256 x, uint256 v) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, v.step(a)));
    }
}
