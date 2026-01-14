// Duplicate mutability specifier
contract C {
    uint immutable immutable x; //~ ERROR: mutability already specified
    uint immutable constant y;  //~ ERROR: mutability already specified
}
