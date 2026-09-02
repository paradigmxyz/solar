//@ codegen-matrix: standard optimized
//@[mir] filecheck: --check-prefix=MIR
//@[optimized] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=OPT
//@ run-call: CodeFactory::deployCreationCode() => 7
//@ run-call: CodeFactory::runtimeCodeMatches() => true

contract CodeTarget {
    function value() external pure returns (uint256) {
        return 7;
    }

    function payload() external pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00";
    }
}

// MIR-LABEL: contract_code_data.sol:CodeFactory ===
// MIR: data:
// MIR: CodeTarget_initcode_0: hex"
// MIR: CodeTarget_runtime_code_[[RUNTIME:[0-9]+]]: hex"
// MIR-NOT: CodeTarget_initcode_1:
// OPT-LABEL: contract_code_data.sol:CodeFactory (runtime) ===
// OPT: @module CodeFactory_runtime
// OPT: push_data CodeTarget_initcode_0+{{[0-9]+}}
// OPT: @data CodeTarget_initcode_0 hex"
// OPT-NOT: @data CodeTarget_runtime_code_
contract CodeFactory {
    // MIR-LABEL: fn @deployCreationCode{{[( ]}}
    // MIR: data_copy CodeTarget_initcode_0,
    function deployCreationCode() external returns (uint256) {
        bytes memory code = type(CodeTarget).creationCode;
        address deployed;
        assembly ("memory-safe") {
            deployed := create(0, add(code, 32), mload(code))
        }
        return CodeTarget(deployed).value();
    }

    // MIR-LABEL: fn @runtimeCodeMatches{{[( ]}}
    // MIR: data_copy CodeTarget_initcode_0,
    // MIR: data_copy CodeTarget_runtime_code_[[RUNTIME]],
    function runtimeCodeMatches() external returns (bool) {
        CodeTarget deployed = new CodeTarget();
        return keccak256(type(CodeTarget).runtimeCode) == address(deployed).codehash;
    }
}
