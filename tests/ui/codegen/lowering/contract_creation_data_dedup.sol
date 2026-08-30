//@ revisions: mir optimized runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR
//@[optimized] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=OPT
//@[runtime] compile-flags: -Ogas
//@[runtime] run-call: Factory::first() => 7
//@[runtime] run-call: Factory::second() => 7
//@[runtime] run-call: Factory::pair() => 14

contract Child {
    function value() external pure returns (uint256) {
        return 7;
    }
}

// MIR-LABEL: contract_creation_data_dedup.sol:Factory ===
// MIR: data:
// MIR: literal_0: hex"
// MIR-NOT: literal_1:
// OPT-LABEL: contract_creation_data_dedup.sol:Factory (runtime) ===
// OPT: @module runtime
// OPT-COUNT-4: push_data literal_0
// OPT: @data literal_0 hex"
// OPT-NOT: @data literal_1
contract Factory {
    // MIR-LABEL: fn @first{{[( ]}}
    // MIR: data_copy literal_0,
    function first() external returns (uint256) {
        return new Child().value();
    }

    // MIR-LABEL: fn @second{{[( ]}}
    // MIR: data_copy literal_0,
    function second() external returns (uint256) {
        return new Child().value();
    }

    // MIR-LABEL: fn @pair{{[( ]}}
    // MIR-COUNT-2: data_copy literal_0,
    function pair() external returns (uint256) {
        Child left = new Child();
        Child right = new Child();
        return left.value() + right.value();
    }
}
