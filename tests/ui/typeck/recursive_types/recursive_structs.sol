struct A { //~ ERROR: recursive struct definition
    A a;
}

struct A1 {
    A[] a;
}

struct B {
    B[] b;
}

struct C { //~ ERROR: recursive struct definition
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
