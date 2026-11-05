//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime-scheduled
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// solc places every `JUMPDEST` under the location active where its tag is placed
// (`Assembly::append`): an external function's entry tag under the function declaration
// (`ContractCompiler::appendFunctionSelector`) and the return tag of an internal call under the
// call expression (`ExpressionCompiler::visit(FunctionCall)`).

// CHECK-LABEL: (scheduled runtime) ===
// CHECK: bb{{[0-9]+}} [entry, invoke=f@[[F:[0-9]+]]..[[FEND:[0-9]+]]] !metadata(span=[[F]]..[[FEND]]):
// CHECK: push bb[[RET:[0-9]+]] !metadata(span=[[CALL:[0-9]+]]..[[CALLEND:[0-9]+]])
// CHECK: bb[[RET]] [continuation] !metadata(span=[[CALL]]..[[CALLEND]]):
contract C {
    uint256 s;

    function twice(uint256 x) internal returns (uint256) {
        s += x;
        return x * 2;
    }

    function f(uint256 a) external returns (uint256 r) {
        r = twice(a);
    }
}
