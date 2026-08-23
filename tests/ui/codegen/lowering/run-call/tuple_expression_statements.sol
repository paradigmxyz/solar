//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
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
