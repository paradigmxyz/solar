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
}

contract C {
    mapping(uint => uint) m0;
    mapping(E => uint) m1;
    mapping(U => uint) m2;
    mapping(L.E1 => uint) m3;
    mapping(uint[] => uint) m4; //~ ERROR: expected `=>`, found `[`
}
