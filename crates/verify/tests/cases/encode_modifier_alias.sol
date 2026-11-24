pragma solidity ^0.8.20;
// A modifier that receives a memory reference may write the object through it, whatever
// expression of the caller's names selected the reference; a value parameter receives a copy.
contract EncodeModifierAlias {
    function _set(uint256[1] memory part) private pure {
        part[0] = 1;
    }

    modifier touch(uint256[1] memory part) {
        _set(part);
        _;
    }

    modifier peek(uint256) {
        _;
    }

    function _step(uint256[1][1] memory a, bool flag) private pure touch(flag ? a[0] : a[0]) returns (uint256 r) {
        r = 2;
    }

    function _look(uint256[1] memory a) private pure peek(a[0]) returns (uint256 r) {
        r = 2;
    }

    // The modifier writes `a[0]` before the compiled code encodes `a`.
    // expect: solver returned unknown
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_modifier_alias(uint256 x, bool flag) external pure returns (bytes32 r) {
        uint256[1][1] memory a;
        a[0][0] = x;
        r = keccak256(abi.encode(a, _step(a, flag)));
    }

    // A word of `a` passed by value leaves the hash exact.
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function ok_modifier_value(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, _look(a)));
    }
}
