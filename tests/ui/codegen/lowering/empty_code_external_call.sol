//@[mir] filecheck:
//@ compile-flags: --libraries EmptyCodeLibrary=0x1111111111111111111111111111111111111111
//@ codegen-matrix: standard
//@ run-call-fail: EmptyCodeCalls::direct => 0x
//@ run-call-fail: EmptyCodeCalls::pointer => 0x
//@ run-call-fail: EmptyCodeCalls::libraryCall => 0x
//@ run-call-fail: EmptyCodeCalls::tryDirect => 0x
//@ run-call-fail: EmptyCodeCalls::tryPointer => 0x
//@ run-call-fail: EmptyCodeCalls::tryStatic => 0x
//@ run-call-fail: EmptyCodeTryFactory::deploy => 0x
//@ run-call: EmptyCodeCalls::lowLevel => true
//@ run-call: EmptyCodeCalls::selfCall => true
//@ run-call: EmptyCodeCalls::trySelf => true

contract EmptyCodeTarget {
    function noop() external {}
}

interface EmptyCodeViewTarget {
    function noop() external view;
}

library EmptyCodeLibrary {
    function noop() external {}
}

contract EmptyCodeCalls {
    // CHECK-LABEL: fn @direct(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function direct() external {
        EmptyCodeTarget(address(0)).noop();
    }

    // CHECK-LABEL: fn @pointer(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function pointer() external {
        function() external target = EmptyCodeTarget(address(0)).noop;
        target();
    }

    // CHECK-LABEL: fn @libraryCall(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function libraryCall() external {
        EmptyCodeLibrary.noop();
    }

    // CHECK-LABEL: fn @tryDirect(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function tryDirect() external {
        try EmptyCodeTarget(address(0)).noop() {} catch {}
    }

    // CHECK-LABEL: fn @tryPointer(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function tryPointer() external {
        function() external target = EmptyCodeTarget(address(0)).noop;
        try target() {} catch {}
    }

    // CHECK-LABEL: fn @tryStatic(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function tryStatic() external view {
        try EmptyCodeViewTarget(address(0)).noop() {} catch {}
    }

    // CHECK-LABEL: fn @trySelf(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function trySelf() external returns (bool) {
        try this.noop() {
            return true;
        } catch {
            return false;
        }
    }

    function lowLevel() external returns (bool success) {
        (success,) = address(0).call("");
    }

    // CHECK-LABEL: fn @selfCall(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    function selfCall() external returns (bool) {
        this.noop();
        return true;
    }

    function noop() external {}
}

contract EmptyCodeTryFactory {
    function deploy() external {
        new EmptyCodeTryConstructor();
    }
}

contract EmptyCodeTryConstructor {
    // CHECK-LABEL: @module EmptyCodeTryConstructor
    // CHECK-LABEL: fn @constructor(
    // CHECK-NOT: extcodesize
    // CHECK: abi_encode []
    // CHECK: extcodesize
    // CHECK: revert_if {{.*}}, target_contract_has_no_code
    constructor() {
        try this.noop() {} catch {}
    }

    function noop() external {}
}
