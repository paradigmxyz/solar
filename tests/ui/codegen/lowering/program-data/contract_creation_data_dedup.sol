//@ codegen-matrix: standard optimized
//@[mir] filecheck: --check-prefix=MIR
//@[optimized] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=OPT
//@ run-call: Factory::first() => 7
//@ run-call: Factory::second() => 7
//@ run-call: Factory::pair() => 14

contract Child {
    function value() external pure returns (uint256) {
        return 7;
    }

    function payload() external pure returns (bytes memory) {
        return hex"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00";
    }
}

// MIR-LABEL: contract_creation_data_dedup.sol:Factory ===
// MIR: data:
// MIR: Child_initcode_0: hex"
// MIR-NOT: Child_initcode_1:
// OPT-LABEL: contract_creation_data_dedup.sol:Factory (runtime) ===
// OPT: @module Factory_runtime
// OPT-COUNT-3: push_data Child_initcode_0
// OPT: @data Child_initcode_0 hex"
// OPT-NOT: @data Child_initcode_1
contract Factory {
    // MIR-LABEL: fn @first{{[( ]}}
    // MIR: data_copy Child_initcode_0,
    function first() external returns (uint256) {
        return new Child().value();
    }

    // MIR-LABEL: fn @second{{[( ]}}
    // MIR: data_copy Child_initcode_0,
    function second() external returns (uint256) {
        return new Child().value();
    }

    // MIR-LABEL: fn @pair{{[( ]}}
    // MIR-COUNT-2: data_copy Child_initcode_0,
    function pair() external returns (uint256) {
        Child left = new Child();
        Child right = new Child();
        return left.value() + right.value();
    }
}
