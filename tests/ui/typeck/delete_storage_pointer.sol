// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/213_no_delete_on_storage_pointers.sol

contract C {
    uint[] data;
    function f() public {
        uint[] storage x = data;
        delete x; //~ ERROR: cannot delete `uint256[] storage`
    }
}

contract D {
    struct S {
        uint x;
        mapping(uint => uint) m;
    }

    uint[] data;
    S s;
    mapping(uint => uint) m;
    mapping(uint => uint)[] ms;

    function f(uint[] storage p) internal returns (S storage r) {
        r = s;
        uint[] storage x = data;

        delete p; //~ ERROR: cannot delete `uint256[] storage`
        delete r; //~ ERROR: cannot delete `struct D.S storage`
        delete (x); //~ ERROR: cannot delete `uint256[] storage`

        delete data;
        delete x[0];
        delete s;
        delete r.x;
        delete m[0];
        delete ms[0][0];
        delete ms;
    }

    function g(bytes calldata c) external {
        mapping(uint => uint) storage mp = m;

        delete m; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete ms[0]; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete s.m; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete mp; //~ ERROR: cannot delete `mapping(uint256 => uint256) storage`
        delete c; //~ ERROR: cannot delete `bytes calldata`
    }
}
