//@ run-call: sum() => 45
//@ run-call: writeThrough() => 7

// A destructuring declaration binds its targets from the statement's initializer, so a mapping
// storage pointer declared there is initialized even though the target itself carries no
// initializer. Writing through the pointer writes the state variable's mapping.
library IL {
    struct S {
        mapping(uint256 => uint256) m;
        uint256 n;
    }

    function both(S storage s)
        internal
        view
        returns (mapping(uint256 => uint256) storage, uint256)
    {
        return (s.m, s.n);
    }
}

contract C {
    IL.S private s;

    function sum() external returns (uint256) {
        s.n = 5;
        (mapping(uint256 => uint256) storage m, uint256 n) = IL.both(s);
        m[1] = 40;
        return m[1] + n;
    }

    function writeThrough() external returns (uint256) {
        (mapping(uint256 => uint256) storage m, ) = IL.both(s);
        m[2] = 7;
        return s.m[2];
    }
}
