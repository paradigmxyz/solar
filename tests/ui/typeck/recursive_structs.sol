struct A { //~ ERROR: recursive struct definition
    //~^ ERROR: recursive struct definition
    // TODO: Cache the check so we don't emit the error twice.
    A a;
}

struct A1 {
    A[] a;
}

struct B {
    B[] b;
}

struct C {
    A a;
}

struct D {
    mapping(uint => D) m;
    F[] f;
}

struct E {
    function(E memory) e;
}

struct F {
    D d;
    E e;
}
