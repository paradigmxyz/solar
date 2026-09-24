//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: next 0 => 1
//@ run-call: next 41 => 42
//@ run-call-fail: next 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: both 0 => 7
//@ run-call: both 42 => 51
//@ run-call: ReturndataBranch::test 0 => 9
//@ run-call: ReturndataBranch::test 1 => 7
//@ run-call: ReturndataBranch::test 32 => 7

contract LiveBranchCondition {
    // CHECK-LABEL: @module LiveBranchCondition_runtime
    // The return store of the assembly switch is shared after dispatcher
    // inlining. The checked sum stays on the stack across its overflow guard
    // until its own return, which the default run count keeps inline rather
    // than sharing through a jump.
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK-NEXT: [[RETURN]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK: push 0xedd004e5
    // CHECK: add
    // CHECK-NOT: mstore
    // CHECK: jumpi
    // CHECK-NOT: mload
    // CHECK: return
    function next(uint256 x) external pure returns (uint256) {
        return x + 1;
    }

    function both(uint256 x) external pure returns (uint256) {
        uint256 result;
        assembly {
            switch x
            case 0 { result := add(x, 7) }
            default { result := add(x, 9) }
        }
        return result;
    }
}

contract ReturndataBranch {
    // CHECK-LABEL: @module ReturndataBranch_runtime
    // CHECK: returndatasize
    // CHECK-NEXT: push {{bb[0-9]+}}
    // CHECK-NEXT: jumpi
    function test(uint256 size) external view returns (uint256) {
        assembly {
            if iszero(staticcall(gas(), 4, 0, size, 0, 0)) { revert(0, 0) }
            if gt(returndatasize(), 0) {
                mstore(0, 7)
                return(0, 32)
            }
            mstore(0, 9)
            return(0, 32)
        }
    }
}
