//@ revisions: mir evmir
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR
//@[evmir] compile-flags: -Zdump=evm-ir,evm-ir-runtime --pretty-json
//@[evmir] filecheck: --check-prefix=EVMIR

contract ConstructorICall {
    // MIR-LABEL: fn @value{{[( ]}}
    // MIR: sload 0
    uint256 public value;

    // MIR-LABEL: fn @constructor{{[( ]}}
    // MIR: [[MASKED:v[0-9]+]] = and arg0, 7
    // MIR: [[VALUE:v[0-9]+]] = icall @helper, 1, [[MASKED]]
    // MIR: sstore 0, [[VALUE]]
    // EVMIR-LABEL: @module ConstructorICall_deployment
    // EVMIR: pop
    // EVMIR-NEXT: push [[CTOR_CONT:bb[0-9]+]]
    // EVMIR-NEXT: jump [[HELPER:bb[0-9]+]]
    // The recursive call edge falls through into the helper's entry test.
    // EVMIR: [[RECURSE_BLOCK:bb[0-9]+]]:
    // EVMIR-NEXT: push 11
    // EVMIR: mul
    // EVMIR: jumpi
    // EVMIR-NEXT: push 1
    // EVMIR: push {{bb[0-9]+}}
    // EVMIR-NEXT: jump [[HELPER]]
    // EVMIR: [[HELPER]]:
    // EVMIR: push [[RECURSE_BLOCK]]
    // EVMIR-NEXT: jumpi
    // EVMIR: [[CTOR_CONT]] [continuation]:
    // EVMIR: sstore
    // EVMIR: return
    // EVMIR-LABEL: @module ConstructorICall_runtime
    // EVMIR: push 0x3fa4f245
    // EVMIR: sload
    // EVMIR: return
    constructor(uint256 x) {
        value = helper(x & 7);
    }

    // MIR-LABEL: fn @helper{{[( ]}}
    // MIR: [[NEXT:v[0-9]+]] = sub arg0, 1
    // MIR: {{v[0-9]+}} = icall @helper, 1, [[NEXT]]
    // MIR: ret
    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 11 + helper(n - 1);
    }
}
