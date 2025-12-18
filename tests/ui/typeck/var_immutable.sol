//@compile-flags: -Ztypeck

contract C {
    // Immutable value types - OK
    uint immutable a;
    address immutable b;
    bool immutable c;
    bytes32 immutable d;
    int256 immutable e;

    // Immutable reference types - ERROR
    uint[] immutable f;          //~ ERROR: immutable variables cannot have a non-value type
    string immutable g;          //~ ERROR: immutable variables cannot have a non-value type
    bytes immutable h;           //~ ERROR: immutable variables cannot have a non-value type

    struct S { uint x; }
    S immutable i;               //~ ERROR: immutable variables cannot have a non-value type

    mapping(uint => uint) immutable j;  //~ ERROR: immutable variables cannot have a non-value type
}
