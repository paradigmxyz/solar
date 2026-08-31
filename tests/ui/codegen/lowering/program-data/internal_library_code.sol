//@ compile-flags: --emit=bin-runtime

library Math {
    function square(uint256 value) internal pure returns (uint256) {
        return value * value;
    }
}

contract UsesMath {
    function runtimeCodeHash() external pure returns (bytes32) {
        return keccak256(type(Math).runtimeCode); //~ ERROR: codegen is missing bytecode
    }
}
