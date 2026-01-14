// Constant not allowed in struct fields
contract C {
    struct S {
        uint constant x; //~ ERROR: mutability is not allowed here
    }
}
