//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f true => 0
//@ run-call: f false => 3
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
