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
//@[none] run-call: Flow::f() => 1, 2
//@[gas] run-call: Flow::f() => 1, 2
//@[size] run-call: Flow::f() => 1, 2
// ported-from: test/libsolidity/semanticTests/functionTypes/struct_with_external_function.sol

struct S {
    uint16 a;
    function() external returns (uint256) pointer;
    uint16 b;
}

contract Flow {
    S[2] values;

    function first() public pure returns (uint256) {
        return 1;
    }

    function second() public pure returns (uint256) {
        return 2;
    }

    constructor() {
        values[0].a = 0xff07;
        values[0].b = 0xff07;
        values[1].pointer = this.second;
        values[1].a = 0xff07;
        values[1].b = 0xff07;
        values[0].pointer = this.first;
    }

    function f() public returns (uint256, uint256) {
        return (values[0].pointer(), values[1].pointer());
    }
}
