//@[mir] filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@[size] compile-flags: -Zdump=disasm-runtime
//@[size] filecheck: --check-prefix=STACK --implicit-check-not=CALLDATALOAD
//@ run-call: chain 12, 3 => 15
//@ run-call-fail: chain 12, 0 => Panic(0x12)
//@ run-call-fail: chain 11, 3
//@ run-call: chain 0, 1 => 1

contract RequireModuloCases {
    // Each argument is read once, in either order, and stays on the stack for both checks.
    // STACK-LABEL: RequireModuloCases (runtime)
    // STACK: CALLDATALOAD
    // STACK-DAG: PUSH1 0x04{{[[:space:]]+}}CALLDATALOAD
    // STACK-DAG: PUSH1 0x24{{[[:space:]]+}}CALLDATALOAD
    // STACK-DAG: CALLDATASIZE
    // STACK: MOD
    // STACK: MOD
    // STACK: RETURN
    function chain(uint256 a, uint256 b) external pure returns (uint256) {
        require(a % b == 0);
        require(b % 3 == 0 || a % 2 == 0);
        return a + b;
    }
}
