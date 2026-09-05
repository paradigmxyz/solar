// Every target of a declaration statement is checked against the return
// component at its index. A call initializer is one expression producing all
// of them, so deriving the targets' expressions from the initializer's syntax
// only checked component 0: a valid first component then left the later ones
// unchecked, and a storage reference could be bound at an unrelated type,
// which makes a write through it land on whatever the declared layout puts
// there.
library Lib {
    struct Actual {
        uint256 a;
        uint256 b;
    }

    function pick(mapping(uint256 => uint256) storage m, Actual storage a)
        internal
        pure
        returns (mapping(uint256 => uint256) storage, Actual storage)
    {
        return (m, a);
    }

    function two(Actual storage a) internal pure returns (uint256, Actual storage) {
        return (a.a, a);
    }
}

struct Fake {
    address owner;
}

contract C {
    address public owner;
    mapping(uint256 => uint256) internal map;
    Lib.Actual internal actual;

    function incompatibleSecond(address n) public {
        (mapping(uint256 => uint256) storage m, Fake storage f) = Lib.pick(map, actual);
        //~^ ERROR: mismatched types
        m[1] = 2;
        f.owner = n;
    }

    function incompatibleValue() public view returns (address) {
        (address a, Lib.Actual storage s) = Lib.two(actual); //~ ERROR: mismatched types
        s;
        return a;
    }

    function droppedSecond() public {
        (mapping(uint256 => uint256) storage m, ) = Lib.pick(map, actual);
        m[1] = 2;
    }

    function compatible() public view returns (uint256) {
        (uint256 v, Lib.Actual storage s) = Lib.two(actual);
        return v + s.b;
    }

    function tupleLiteral() public view returns (uint256) {
        (uint256 v, Lib.Actual storage s) = (actual.a, actual);
        return v + s.b;
    }

    function tupleLiteralIncompatible() public view returns (uint256) {
        (uint256 v, Fake storage f) = (actual.a, actual); //~ ERROR: mismatched types
        f;
        return v;
    }
}
