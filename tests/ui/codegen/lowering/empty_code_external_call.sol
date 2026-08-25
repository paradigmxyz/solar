//@ filecheck:
// CHECK: @module
//@ compile-flags: --libraries EmptyCodeLibrary=0x1111111111111111111111111111111111111111
//@ codegen-matrix: standard
//@ run-call-fail: EmptyCodeCalls::direct() => 0x
//@ run-call-fail: EmptyCodeCalls::pointer() => 0x
//@ run-call-fail: EmptyCodeCalls::libraryCall() => 0x
//@ run-call-fail: EmptyCodeCalls::tryDirect() => 0x
//@ run-call-fail: EmptyCodeCalls::tryPointer() => 0x
//@ run-call-fail: EmptyCodeCalls::tryStatic() => 0x
//@ run-call-fail: EmptyCodeTryFactory::deploy() => 0x
//@ run-call: EmptyCodeCalls::lowLevel() => true
//@ run-call: EmptyCodeCalls::selfCall() => true
//@ run-call: EmptyCodeCalls::trySelf() => true

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
    function direct() external {
        EmptyCodeTarget(address(0)).noop();
    }

    function pointer() external {
        function() external target = EmptyCodeTarget(address(0)).noop;
        target();
    }

    function libraryCall() external {
        EmptyCodeLibrary.noop();
    }

    function tryDirect() external {
        try EmptyCodeTarget(address(0)).noop() {} catch {}
    }

    function tryPointer() external {
        function() external target = EmptyCodeTarget(address(0)).noop;
        try target() {} catch {}
    }

    function tryStatic() external view {
        try EmptyCodeViewTarget(address(0)).noop() {} catch {}
    }

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
    constructor() {
        try this.noop() {} catch {}
    }

    function noop() external {}
}
