contract C {
    struct S {
        uint x;
        mapping(uint => uint) m;
    }

    S s;
    mapping(uint => uint) m;
    mapping(uint => uint)[] ms;

    function f() public {
        delete m; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete ms[0]; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete s.m; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`

        delete m[0];
        delete ms;
        delete s;
    }
}
