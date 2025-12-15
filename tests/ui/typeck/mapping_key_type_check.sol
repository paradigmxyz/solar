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
    mapping(bytes => uint) m1b;
    mapping(E => uint) m2;
    mapping(U => uint) m3;
    mapping(L.E1 => uint) m4;
    mapping(C => uint) m4b;
    mapping(L.S1 => uint) m5; //~ ERROR: only elementary types, user defined value types, contract types or enums are allowed as mapping keys.
    mapping(uint[] => uint) m6; //~ ERROR: only elementary types, user defined value types, contract types or enums are allowed as mapping keys.

    function access(uint u, string memory s, bytes memory b, E e, U ud, L.E1 e1, C c) public {
        m0[u];
        m1[s];
        m1b[b];
        m2[e];
        m3[ud];
        m4[e1];
        m4b[c];
    }
}
