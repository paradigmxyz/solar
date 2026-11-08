//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck: --check-prefix=IR
//@ run-call: CallStatusBranchStack::first 7 => 8
//@ run-call: CallStatusBranchStack::second 7 => 16
//@ run-call: CallStatusBranchStack::third 7 => 21

// A call that returns a static struct writes it to a buffer whose pointer is both a call operand
// and the argument of the decoder the callers share. Dropping the dead copies of the other call
// operands from under the status word leaves it below that pointer. The status branch swaps it to
// the top for `JUMPI`, so the pointer stays on the stack across the branch and no spill slot
// reload reaches the decoder call.
contract CallStatusBranchStack {
    struct Data {
        uint256 a;
        uint256 b;
        uint256 c;
        uint256 d;
    }

    CallStatusBranchStack internal immutable target;

    constructor() {
        target = this;
    }

    function get(uint256 x) external pure returns (Data memory) {
        return Data(x + 1, x + 2, x + 3, x + 4);
    }

    // IR: staticcall
    // IR-NEXT: iszero
    // IR-NEXT: push [[REVERT:bb[0-9]+]]
    // IR-NEXT: jumpi
    // IR-NOT: mload
    // IR: jump [[DECODE:bb[0-9]+]]
    function first(uint256 x) external view returns (uint256) {
        return target.get(x).a;
    }

    // IR: staticcall
    // IR-NEXT: iszero
    // IR-NEXT: push [[REVERT]]
    // IR-NEXT: jumpi
    // IR-NOT: mload
    // IR: jump [[DECODE]]
    function second(uint256 x) external view returns (uint256) {
        Data memory data = target.get(x);
        return data.a + data.b - 1;
    }

    // IR: staticcall
    // IR-NEXT: iszero
    // IR-NEXT: push [[REVERT]]
    // IR-NEXT: jumpi
    // IR-NOT: mload
    // IR: jump [[DECODE]]
    function third(uint256 x) external view returns (uint256) {
        Data memory data = target.get(x);
        return data.c + data.d;
    }
}
