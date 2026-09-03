//@ codegen-matrix: standard optimized
//@[mir] filecheck: --check-prefix=MIR
//@[optimized] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[optimized] filecheck: --check-prefix=OPT
//@ run-call: FactoryWithArgs::plain => 11
//@ run-call: FactoryWithArgs::salted => 22
//@ run-call: FactoryWithArgs::pair => 7

contract ChildWithArg {
    uint256 public immutable value;

    constructor(uint256 value_) {
        value = value_;
    }

    function payload() external pure returns (bytes memory) {
        return hex"ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";
    }
}

// MIR-LABEL: contract_creation_args_data_dedup.sol:FactoryWithArgs ===
// MIR: data:
// MIR: ChildWithArg_initcode_0: hex"
// MIR-NOT: ChildWithArg_initcode_1:
// OPT-LABEL: contract_creation_args_data_dedup.sol:FactoryWithArgs (runtime) ===
// OPT: @module FactoryWithArgs_runtime
// OPT-COUNT-4: push_data ChildWithArg_initcode_0
// OPT: @data ChildWithArg_initcode_0 hex"
// OPT-NOT: @data ChildWithArg_initcode_1
contract FactoryWithArgs {
    // MIR-LABEL: fn @plain{{[( ]}}
    // MIR: data_copy ChildWithArg_initcode_0,
    function plain() external returns (uint256) {
        return new ChildWithArg(11).value();
    }

    // MIR-LABEL: fn @salted{{[( ]}}
    // MIR: data_copy ChildWithArg_initcode_0,
    function salted() external returns (uint256) {
        return new ChildWithArg{salt: bytes32(uint256(1))}(22).value();
    }

    // MIR-LABEL: fn @pair{{[( ]}}
    // MIR-COUNT-2: data_copy ChildWithArg_initcode_0,
    function pair() external returns (uint256) {
        ChildWithArg left = new ChildWithArg(3);
        ChildWithArg right = new ChildWithArg(4);
        return left.value() + right.value();
    }
}
