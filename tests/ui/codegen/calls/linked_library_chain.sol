//@compile-flags: -Zcodegen --libraries Inner=0x1000000000000000000000000000000000000001,Outer=0x1000000000000000000000000000000000000002 --emit=bin-runtime

// A linked library may itself call another linked library, forwarding a
// storage-reference parameter: the storage struct travels as its slot through
// BOTH delegatecall hops and still addresses the original caller's storage.
// `C.go` delegatecalls `Outer.callThrough`, which delegatecalls `Inner.bump`.
// Runtime behavior (return values and the mutation of `C`'s storage) is
// verified equal to solc 0.8.30's linked flow separately.

library DataTypes {
    struct Map {
        uint256 data;
    }
}

library Inner {
    function bump(DataTypes.Map storage m, uint256 by) public returns (uint256) {
        m.data += by;
        return m.data;
    }
}

library Outer {
    function callThrough(DataTypes.Map storage m, uint256 by) public returns (uint256) {
        if (by == 0) {
            return 0;
        }
        return Inner.bump(m, by);
    }
}

contract C {
    DataTypes.Map map;

    function go(uint256 by) external returns (uint256) {
        return Outer.callThrough(map, by);
    }
}
