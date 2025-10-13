contract B {
    uint x;
    uint y;
}
contract C {
    mapping(B.x => uint) m0; // Identifier path as mapping key type is allowed
    mapping(B.x => mapping(B.x => B.y)) m1;
    mapping([] => uint) m2; //~ ERROR: expected one of elementary type name or path, found `[`
}


