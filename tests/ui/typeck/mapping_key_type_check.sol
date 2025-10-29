//@compile-flags: -Ztypeck
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
    mapping(string => uint) m1;
    mapping(E => uint) m2;
    mapping(U => uint) m3;
    mapping(L.E1 => uint) m4;
    mapping(L.S1 => uint) m5; //~ ERROR: only elementary types, user defined value types, contract types or enums are allowed as mapping keys.
}
