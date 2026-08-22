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
//@[none] run-call: f true => 0
//@[gas] run-call: f true => 0
//@[size] run-call: f true => 0
//@[none] run-call: f false => 3
//@[gas] run-call: f false => 3
//@[size] run-call: f false => 3
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_local_variables.sol

contract ModifierLocalVariables {
    modifier initializeLocals() {
        uint8 a = 1;
        uint8 b = 2;
        a;
        b;
        _;
    }

    modifier skip(bool condition) {
        if (condition) return;
        _;
    }

    function f(bool condition)
        external
        pure
        initializeLocals
        skip(condition)
        returns (uint256 result)
    {
        result = 3;
    }
}
