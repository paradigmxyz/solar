//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: highLevelGas() => true

contract TryCallOptionsTarget {
    function ping() external {}
}

contract TryCallOptions {
    TryCallOptionsTarget private target;

    constructor() {
        target = new TryCallOptionsTarget();
    }

    function highLevelGas() external returns (bool) {
        try target.ping{gas: 0}() {
            return false;
        } catch {
            return true;
        }
    }
}
