//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/inheritance/override/override_implemented_and_unimplemented_with_implemented_call_via_contract.sol

contract A {
    function f() public virtual {}
}

abstract contract B {
    function f() public virtual;
}

contract C is A, B {
    function f() public virtual override(A, B) {
        B.f(); //~ ERROR: cannot call unimplemented base function
    }
}
