// Selectors only fold for items that have one, and whose signature has ABI types, unlike recursive
// structs outside of libraries. Type checking reports the others.
struct A {
    B[] b;
}

struct B {
    A[] a;
}

error E(A); //~ ERROR: recursive types cannot be error parameter types

interface I {
    function f(A calldata) external; //~ ERROR: recursive types cannot be parameter or return types of public functions
}

contract C {
    function internalFn(A memory) internal pure {}

    uint[internalFn.selector.length] internalSelector; //~ ERROR: failed to evaluate constant: unsupported expression
    //~^ ERROR: member `selector` not found
    uint[E.selector.length] errorSelector; //~ ERROR: failed to evaluate constant: unsupported expression
    // The interface ID leaves out the function with a recursive type.
    uint[type(I).interfaceId.length] interfaceId;
}

library L {
    struct S {
        S[] children;
    }

    function f(S storage) public {}
}

contract D {
    uint[L.f.selector.length] librarySelector;
}
