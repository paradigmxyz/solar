//@ revisions: mir evmir
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck: --check-prefix=MIR
//@[evmir] compile-flags: -Zdump=evm-ir --pretty-json
//@[evmir] filecheck: --check-prefix=EVMIR

contract ConstructorInternalCall {
    // MIR-LABEL: fn @value{{[( ]}}
    // MIR: sload 0
    uint256 public value;

    // MIR-LABEL: fn @constructor{{[( ]}}
    // MIR: [[MASKED:v[0-9]+]] = and arg0, 7
    // MIR: [[VALUE:v[0-9]+]] = internal_call @helper, 1, [[MASKED]]
    // MIR: sstore 0, [[VALUE]]
    // EVMIR-LABEL: @module ConstructorInternalCall_deployment
    // EVMIR: push 7
    // EVMIR-NEXT: and
    // EVMIR-NEXT: push [[CTOR_CONT:bb[0-9]+]]
    // EVMIR-NEXT: swap 1
    // EVMIR-NEXT: jump [[HELPER_ENTRY:bb[0-9]+]]
    // EVMIR: [[HELPER_ENTRY]]:
    // EVMIR-NEXT: jump [[HELPER:bb[0-9]+]]
    // EVMIR: [[HELPER]]:
    // EVMIR: jumpi [[RECURSE_BLOCK:bb[0-9]+]], {{bb[0-9]+}}
    // EVMIR: [[CTOR_CONT]]:
    // EVMIR: sstore
    // EVMIR: codecopy
    // EVMIR: return
    // EVMIR: [[RECURSE_BLOCK]]:
    // EVMIR-NEXT: push 11
    // EVMIR: mul
    // EVMIR: jumpi
    // EVMIR: push 1
    // EVMIR: sub
    // EVMIR-NEXT: push {{bb[0-9]+}}
    // EVMIR-NEXT: swap 1
    // EVMIR-NEXT: jump [[HELPER_ENTRY]]
    // EVMIR-LABEL: @module ConstructorInternalCall_runtime
    // EVMIR: push 0x3fa4f245
    // EVMIR: sload
    // EVMIR: return
    constructor(uint256 x) {
        value = helper(x & 7);
    }

    // MIR-LABEL: fn @helper{{[( ]}}
    // MIR: [[NEXT:v[0-9]+]] = sub arg0, 1
    // MIR: {{v[0-9]+}} = internal_call @helper, 1, [[NEXT]]
    // MIR: ret
    function helper(uint256 n) internal pure returns (uint256) {
        if (n == 0) {
            return 1;
        }
        return n * 11 + helper(n - 1);
    }
}
