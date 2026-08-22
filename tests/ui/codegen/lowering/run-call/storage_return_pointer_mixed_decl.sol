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
//@[none, gas, size] run-call: f() => true
// ported-from: test/libsolidity/semanticTests/various/typed_multi_variable_declaration.sol

contract StorageReturnPointerMixedDecl {
    struct S {
        uint256 x;
    }

    S s;

    function g() internal returns (uint256, S storage, uint256) {
        s.x = 7;
        return (1, s, 2);
    }

    function f() public returns (bool) {
        (uint256 x1, S storage y1, uint256 z1) = g();
        if (x1 != 1 || y1.x != 7 || z1 != 2) return false;
        (, S storage y2, ) = g();
        if (y2.x != 7) return false;
        (uint256 x2, , ) = g();
        if (x2 != 1) return false;
        (, , uint256 z2) = g();
        if (z2 != 2) return false;
        return true;
    }
}
