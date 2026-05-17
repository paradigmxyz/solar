//@compile-flags: -Ztypeck

struct S {
    uint256 x;
}

function inc(uint256 x) pure returns (uint256) {
    return x + 1;
}

function field(S memory s) pure returns (uint256) {
    return s.x;
}

library L {
    function twice(uint256 x) internal pure returns (uint256) {
        return x * 2;
    }

    function add(uint256 x, uint256 y) public pure returns (uint256) {
        return x + y;
    }
}

using {inc} for uint256;
using {field} for S global;

contract C {
    using L for uint256;

    function f(S memory s) public pure {
        uint256 x = 1;
        x.inc();
        x.twice();
        x.add(2);
        s.field();
    }
}
