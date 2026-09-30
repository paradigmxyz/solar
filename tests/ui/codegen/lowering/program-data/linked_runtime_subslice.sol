//@ revisions: unoptimized optimized
//@[unoptimized] compile-flags: -O none -Zdump=evm-ir-runtime
//@[unoptimized] filecheck: --check-prefix=NONE
//@[optimized] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=GAS
//@ run-call: Factory::runtimeMatches => true

contract Child {
    function v() external pure returns (uint256) {
        return 7;
    }
}

// Linked runtime code shares the creation code that contains it only when the
// optimizer allows data subslices.
// NONE-LABEL: @module Factory_runtime
// NONE: Child_runtime_code_1: hex
// NONE: push_data Child_creation_code_0
// NONE: push_data Child_runtime_code_1
// GAS-LABEL: @module Factory_runtime
// GAS: push_data Child_creation_code_0+{{[0-9]+}}
// GAS-NOT: Child_runtime_code
contract Factory {
    function creation() external pure returns (bytes memory) {
        return type(Child).creationCode;
    }

    function runtimeMatches() external returns (bool) {
        return keccak256(type(Child).runtimeCode) == keccak256(address(new Child()).code);
    }
}
