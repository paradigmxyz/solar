// SPDX-License-Identifier: MIT
contract C {
    // Struct with only dynamic array field - gets filtered, leaving no returns
    struct R {
        R[] r;
    }

    R public myVar; //~ ERROR: getter must return at least one value

    // Struct with a non-array recursive field
    struct S {
        uint x;
        S[] s;
    }

    // This should work since x is returned (s is filtered as array)
    S public myVar2;

    // Direct recursive struct as function parameter - this tests the recursive type check
    function testRecursive(S memory s) public {} //~ ERROR: recursive types cannot be parameter or return types of public functions
}
