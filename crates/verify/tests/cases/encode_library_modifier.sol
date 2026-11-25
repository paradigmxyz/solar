pragma solidity ^0.8.20;
// A library function's modifiers resolve in the library, not in the contract that calls it.
library Steps {
    function _set(uint256[1] memory part) private pure {
        part[0] = 1;
    }

    modifier touch(uint256[1] memory part) {
        _set(part);
        _;
    }

    function step(uint256[1] memory a) internal pure touch(a) returns (uint256 r) {
        r = 2;
    }
}

contract EncodeLibraryModifier {
    // Same name as the library's, but takes a value.
    modifier touch(uint256) {
        _;
    }

    // `Steps.touch` writes `a[0]` before the compiled code encodes `a`.
    // The modifier's parameter may share the caller's memory, and the callee writes it: refused.
    // expect: may share its memory with another reference
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_library_modifier(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, Steps.step(a)));
    }
}
