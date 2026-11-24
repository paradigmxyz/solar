pragma solidity ^0.8.20;
// A modifier argument the caller's names only feed into a fresh allocation hands the modifier
// no reference to them.
contract EncodeModifierFresh {
    modifier peek(bytes memory) {
        _;
    }

    function _look(uint256[1] memory a) private pure peek(abi.encode(a[0])) returns (uint256 r) {
        r = 2;
    }

    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function ok_modifier_fresh(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, _look(a)));
    }
}
