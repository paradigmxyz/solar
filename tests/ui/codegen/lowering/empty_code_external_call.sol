//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@ compile-flags: --libraries EmptyCodeLibrary=0x1111111111111111111111111111111111111111
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call-fail: EmptyCodeCalls::direct() => 0x
//@ run-call-fail: EmptyCodeCalls::pointer() => 0x
//@ run-call-fail: EmptyCodeCalls::libraryCall() => 0x
//@ run-call: EmptyCodeCalls::lowLevel() => true
//@ run-call: EmptyCodeCalls::selfCall() => true

contract EmptyCodeTarget {
    function noop() external {}
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

    function lowLevel() external returns (bool success) {
        (success,) = address(0).call("");
    }

    function selfCall() external returns (bool) {
        this.noop();
        return true;
    }

    function noop() external {}
}
