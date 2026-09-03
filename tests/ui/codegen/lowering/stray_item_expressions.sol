//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f 33 => 42
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
