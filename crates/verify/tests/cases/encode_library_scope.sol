pragma solidity ^0.8.20;
// A library function's unqualified calls run in the library: a caller helper of the same name
// and arity does not stand in for the library's writer.
library L {
    function _set(uint256[1] memory a) internal pure returns (uint256) {
        a[0] = 1;
        return 2;
    }

    function step(uint256[1] memory a) internal pure returns (uint256) {
        return _set(a);
    }
}

contract EncodeLibraryScope {
    function _set(uint256[1] memory) private pure returns (uint256) {
        return 2;
    }

    // `L.step` writes `a[0]` through `L._set` before the compiled code encodes `a`.
    // expect: postcondition not satisfied
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_library_scope(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, L.step(a)));
    }
}
