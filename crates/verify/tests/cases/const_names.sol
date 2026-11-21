pragma solidity ^0.8.20;
// Constants are keyed by bare name: `N` sizes no array once two contracts give it different
// values, so `D`'s `N` cannot make `uint256[N]` here a one-word encoding.
contract C {
    uint256 constant N = 2;
    // expect: solver returned unknown
    /// @custom:ensures result
    function bad_named_size(uint256[N] calldata xs) external pure returns (bool) {
        return keccak256(abi.encode(xs)) == keccak256(abi.encode(xs[0]));
    }
}
contract D { uint256 constant N = 1; }
