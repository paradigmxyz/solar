//@compile-flags: -Ztypeck

function inc(uint256 self) pure returns (uint256) {
    return self + 1;
}

function add(uint256 self, uint256 x) pure returns (uint256) {
    return self + x;
}

library L {
    function pick(uint256 self, bool x) internal pure returns (bool) {
        self;
        return x;
    }

    function pick(uint256 self, uint256 x) internal pure returns (uint256) {
        self;
        return x;
    }
}

using {inc, add} for uint256;

contract C {
    using L for uint256;

    function ok(uint256 x, bool b) public pure {
        uint256 a = x.inc();
        uint256 c = x.add(1);
        bool d = x.pick(b);
        uint256 e = x.pick(1);
        a; c; d; e;
    }

    function bad(uint256 x) public pure {
        x.inc; //~ ERROR: attached function `inc` can only be called
        x.pick; //~ ERROR: attached function `pick` can only be called
        x.inc(1); //~ ERROR: wrong argument count for function call
        x.add(); //~ ERROR: wrong argument count for function call
    }
}
