//@ revisions: gas size
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@ filecheck: --implicit-check-not=msize

// Values computed before a low-memory copy stay on the stack across it. The spill store that
// codegen first emits for them is dead, so it neither survives nor asks for a dynamic spill
// base.
// https://github.com/paradigmxyz/solar/issues/1625

contract C {
    // CHECK-LABEL: C (runtime)
    // CHECK: calldatacopy
    // CHECK-NOT: mstore
    // CHECK: return
    fallback() external {
        uint256 v7;
        assembly { v7 := calldatasize() }
        uint256 a = v7 ^ 5;
        uint256 b = v7 + 9;
        assembly { calldatacopy(0, 0, calldatasize()) }
        uint256 res = a + b;
        assembly {
            return(0, add(res, 0))
        }
    }
}
