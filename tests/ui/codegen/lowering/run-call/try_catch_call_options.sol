//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: highLevelGas() => true

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
