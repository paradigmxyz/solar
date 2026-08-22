//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: tupleLiteral() => true
//@ run-call: tupleTernary() => true, 2
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
