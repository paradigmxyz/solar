//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call: f(bool) false => true
//@[none, gas, size] run-call: f(bool) true => false

contract TryCreationChild {
    constructor(bool fail) {
        require(!fail, "x");
    }
}

contract TryCreation {
    function f(bool fail) external returns (bool) {
        try new TryCreationChild(fail) returns (TryCreationChild child) {
            return address(child) != address(0);
        } catch {
            return false;
        }
    }
}
