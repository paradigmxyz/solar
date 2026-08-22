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
//@[none] run-call: C::getData() => 6
//@[gas] run-call: C::getData() => 6
//@[size] run-call: C::getData() => 6
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_for_constructor.sol

contract A {
    uint256 data;

    constructor() mod1 {
        data |= 2;
    }

    modifier mod1 virtual {
        data |= 1;
        _;
    }

    function getData() public returns (uint256 r) {
        //~^ WARN: function state mutability can be restricted to view
        return data;
    }
}

contract C is A {
    modifier mod1 override {
        data |= 4;
        _;
    }
}
