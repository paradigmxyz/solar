pragma solidity ^0.8.20;
// Modifier arguments run in the function's scope before its body: a write there changes an
// object an earlier `abi.encode` argument refers to before the compiled code encodes it.
contract EncodeModifierArgs {
    modifier before(uint256) {
        _;
    }

    function _write(uint256[1] memory a) private pure returns (uint256) {
        a[0] = 1;
        return 2;
    }

    function _step(uint256[1] memory a) private pure before(_write(a)) returns (uint256 r) {
        r = 2;
    }

    function _calm(uint256[1] memory) private pure before(3) returns (uint256 r) {
        r = 2;
    }

    // Only the modifier argument of `_step` writes `a`.
    // expect: solver returned unknown
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_modifier_arg_write(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, _step(a)));
    }

    // A modifier argument that never reaches `a` leaves the hash exact.
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function ok_modifier_arg_calm(uint256 x) external pure returns (bytes32 r) {
        uint256[1] memory a;
        a[0] = x;
        r = keccak256(abi.encode(a, _calm(a)));
    }
}
