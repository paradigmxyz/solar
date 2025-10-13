contract C {
    struct Custom { uint x; }
    mapping(uint => uint) m0;
    mapping(Custom => int) m1;
    mapping(uint[] => uint) m2; //~ ERROR: expected `=>`, found `[`
}


