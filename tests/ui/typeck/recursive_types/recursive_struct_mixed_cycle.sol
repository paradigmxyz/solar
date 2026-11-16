// A direct cycle makes a struct infinitely sized even if it also has an indirect one.

struct A { //~ ERROR: recursive struct definition
    A[] xs;
    B b;
}

struct B { //~ ERROR: recursive struct definition
    A a;
}

struct C { //~ ERROR: recursive struct definition
    mapping(uint => C) m;
    C[][2] xs;
    C[2] c;
}
