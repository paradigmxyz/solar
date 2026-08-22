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
//@[none, gas, size] run-call: f 33 => 42
// ported-from: test/libsolidity/semanticTests/libraries/library_enum_as_an_expression.sol
// ported-from: test/libsolidity/semanticTests/libraries/library_stray_values.sol
// ported-from: test/libsolidity/semanticTests/libraries/library_struct_as_an_expression.sol

library Lib {
    enum Kind {
        A,
        B
    }

    struct Item {
        uint256 value;
    }

    function multiply(uint256 x, uint256 y) public pure returns (uint256) {
        return x * y;
    }
}

contract StrayItemExpressions {
    function f(uint256 x) external pure returns (uint256) {
        Lib;
        Lib.Kind;
        Lib.Item;
        Lib.multiply;
        return x + 9;
    }
}
