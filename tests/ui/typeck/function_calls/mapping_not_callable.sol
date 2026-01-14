//@ compile-flags: -Ztypeck
contract C {
    mapping (uint => uint) m;
    uint a = m(1000); //~ ERROR: expected function
}
