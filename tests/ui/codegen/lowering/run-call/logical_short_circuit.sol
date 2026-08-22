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
//@[none, gas, size] run-call: andSkipsRhs() => 0, false
//@[none, gas, size] run-call: orSkipsRhs() => 0, true
//@[none, gas, size] run-call: andRunsRhs() => 1, true
//@[none, gas, size] run-call: orRunsRhs() => 1, true

contract LogicalShortCircuit {
    uint256 calls;

    function bump() external returns (bool) {
        calls++;
        return true;
    }

    function andSkipsRhs() external returns (uint256, bool) {
        bool result = false && this.bump();
        return (calls, result);
    }

    function orSkipsRhs() external returns (uint256, bool) {
        bool result = true || this.bump();
        return (calls, result);
    }

    function andRunsRhs() external returns (uint256, bool) {
        bool result = true && this.bump();
        return (calls, result);
    }

    function orRunsRhs() external returns (uint256, bool) {
        bool result = false || this.bump();
        return (calls, result);
    }
}
