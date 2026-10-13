// Function type parameter names live in the function type's own scope, hidden from the
// enclosing scope.

contract C {
    uint constant K = 7;

    struct S {
        function(uint a) external f;
        //~^ WARN: named function type parameters are deprecated
        uint a;
    }

    event E(function(uint a) external f, uint a);
    //~^ WARN: named function type parameters are deprecated

    function f() public pure returns (uint) {
        function(bool K) external cb;
        //~^ WARN: named function type parameters are deprecated
        cb;
        uint[K] memory a;
        return K + a.length;
    }

    function g(uint R, function(bool R) external cb) public pure returns (uint) {
        //~^ WARN: named function type parameters are deprecated
        cb;
        return R;
    }

    function h(function(bool K) external cb, bool K) public pure returns (bool) {
        //~^ WARN: named function type parameters are deprecated
        cb;
        return K;
    }

    function i(function(uint a, uint a) external cb) public pure {
        //~^ WARN: named function type parameters are deprecated
        //~| WARN: named function type parameters are deprecated
        //~| ERROR: identifier `a` already declared
        cb;
    }
}
