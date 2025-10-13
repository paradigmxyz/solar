type U is int;
enum E {
    A,
    B
}

library L{
    enum E1 {
        A,
        B
    }
    struct S1 {
        int x;
    }
}


contract C {
    mapping(uint => uint) m0;
    mapping(E => uint) m1;
    mapping(U => uint) m2;
    mapping(L.E1 => uint) m3;

    // TODO: enable typeck feature to trigger this error
    mapping(L.S1 => uint) m4;
}


