// Array lengths cannot read selectors or interface IDs, which solc's evaluator does not compute,
// whether or not the signatures have ABI types.
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
    uint[type(I).interfaceId.length] interfaceId; //~ ERROR: failed to evaluate constant: unsupported expression
}

library L {
    struct S {
        S[] children;
    }

    function f(S storage) public {}
}

contract D {
    uint[L.f.selector.length] librarySelector; //~ ERROR: failed to evaluate constant: unsupported expression
}
