//@ compile-flags: --emit=bin-runtime
//@ run-call: runtimeCodeHash => 0x2451445de446d278512ff1eedde6f7cdfd6a01b16d0a0de35d2f60e96e15280f

library Math {
    function square(uint256 value) internal pure returns (uint256) {
        return value * value;
    }
}

contract UsesMath {
    function runtimeCodeHash() external pure returns (bytes32) {
        return keccak256(type(Math).runtimeCode);
    }
}
