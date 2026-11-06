// https://github.com/paradigmxyz/solar/issues/2429
// Sema must not see the nameless variable that recovery of an old-style fallback leaves behind.

library L {
    function() external {} //~ ERROR: expected a state variable declaration
    // Parameterized recovery still yields a nameless state variable.
    function (uint256 x) public {} //~ ERROR: expected a state variable declaration
    //~^ WARN: named function type parameters are deprecated
    function h() internal {}
}

interface I {
    function() external {} //~ ERROR: expected a state variable declaration
}

contract B {
    function() external payable {} //~ ERROR: expected a state variable declaration
    function k() internal {}
}

contract C is B {
    using L for uint256;

    function f(uint256 v, I i, B b) public {
        L.g(); //~ ERROR: member `g` not found on type `type(library L)`
        v.g(); //~ ERROR: member `g` not found on type `uint256`
        i.g(); //~ ERROR: member `g` not found on type `contract I`
        b.g(); //~ ERROR: member `g` not found on type `contract B`
        function() internal p = L.h;
        p();
    }
}
