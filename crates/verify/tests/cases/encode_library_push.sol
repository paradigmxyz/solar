pragma solidity ^0.8.20;
// A bound library method named like an array builtin (`push`, `pop`) is a call: its writes
// to a memory argument count before an earlier `abi.encode` argument is hashed.
library L {
    function push(uint256, uint256[1] memory a) internal pure returns (uint256) {
        a[0] = 1;
        return 2;
    }
}

contract EncodeLibraryPush {
    using L for uint256;

    // The library `push` writes `a[0]` before the compiled code encodes `a`.
    // expect: postcondition not satisfied
    /// @custom:ensures result == keccak256(abi.encode(uint256(0), uint256(2)))
    function bad_library_push() external pure returns (bytes32 result) {
        uint256[1] memory a;
        return keccak256(abi.encode(a, uint256(0).push(a)));
    }
}
