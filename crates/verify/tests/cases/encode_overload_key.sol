pragma solidity ^0.8.20;
// Overloads of one name and arity are different functions: each one's storage writes count
// before an earlier `abi.encode` argument is hashed from its words.
library Bumps {
    function bump(uint256[1] storage, bool) internal returns (uint256) {
        return 2;
    }

    function bump(uint256[1] storage s, uint256) internal returns (uint256) {
        s[0] = 1;
        return 2;
    }
}

contract EncodeOverloadKey {
    using Bumps for uint256[1];

    uint256[1] internal st;

    // The `uint256` overload writes `st[0]` before the compiled code encodes `st`.
    // expect: solver returned unknown
    /// @custom:ensures r == keccak256(abi.encode(x, uint256(2)))
    function bad_overload_key(uint256 x, uint256 v) external returns (bytes32 r) {
        st[0] = x;
        r = keccak256(abi.encode(st, st.bump(v)));
    }
}
