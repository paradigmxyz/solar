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
//@[none] run-call: Test::f() => 0x202
//@[gas] run-call: Test::f() => 0x202
//@[size] run-call: Test::f() => 0x202
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_library_inheritance.sol

library L {
    struct S {
        uint256 v;
    }

    modifier mod(S storage s) {
        s.v++;
        _;
    }

    function libFun(S storage s) internal mod(s) {
        s.v += 0x100;
    }
}

contract Test {
    using L for *;
    L.S s;

    modifier mod(L.S storage) {
        revert();
        _;
    }

    function f() public returns (uint256) {
        s.libFun();
        L.libFun(s);
        return s.v;
    }
}
