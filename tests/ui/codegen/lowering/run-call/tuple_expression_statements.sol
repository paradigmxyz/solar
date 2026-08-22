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
//@[none] run-call: tupleLiteral() => true
//@[gas] run-call: tupleLiteral() => true
//@[size] run-call: tupleLiteral() => true
//@[none] run-call: tupleTernary() => true, 2
//@[gas] run-call: tupleTernary() => true, 2
//@[size] run-call: tupleTernary() => true, 2
// ported-from: test/libsolidity/semanticTests/expressions/tuple_from_ternary_expression.sol
contract C {
    function tupleLiteral() external pure returns (bool) {
        bool flag;
        ((flag = true), 1);
        return flag;
    }

    function tupleTernary() external pure returns (bool, uint256) {
        bool flag;
        uint256 selected;
        ((flag = true) ? (selected = 2, 3) : (selected = 4, 5));
        return (flag, selected);
    }
}
