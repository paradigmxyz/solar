//@ compile-flags: --emit=bin-runtime
//@ run-call: runtimeCodeHash => 0x3c55237b3869f93f3e570793afec9785f20a4ee7cd0a7798a418838c833228e0

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
