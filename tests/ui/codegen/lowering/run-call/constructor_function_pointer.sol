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
//@[none] run-call: f() => 16
//@[gas] run-call: f() => 16
//@[size] run-call: f() => 16
// ported-from: test/libsolidity/semanticTests/constructor/constructor_function_complex.sol

contract Target {
    uint256 public value;

    constructor(function() external pure returns (uint256) callback) {
        value = callback();
    }
}

contract Caller {
    function f() external returns (uint256) {
        Target target = new Target(this.sixteen);
        return target.value();
    }

    function sixteen() external pure returns (uint256) {
        return 16;
    }
}
