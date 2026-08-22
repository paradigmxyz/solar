//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@ compile-flags: --libraries EmptyCodeLibrary=0x1111111111111111111111111111111111111111
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call-fail: EmptyCodeCalls::direct() => 0x
//@[none, gas, size] run-call-fail: EmptyCodeCalls::pointer() => 0x
//@[none, gas, size] run-call-fail: EmptyCodeCalls::libraryCall() => 0x
//@[none, gas, size] run-call: EmptyCodeCalls::lowLevel() => true
//@[none, gas, size] run-call: EmptyCodeCalls::selfCall() => true

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
