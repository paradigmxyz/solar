contract C {
    struct S {
        uint constant a; //~ ERROR: mutability is not allowed here
        uint immutable b; //~ ERROR: mutability is not allowed here
    }
}
