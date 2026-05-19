//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_functions_inside_contract.sol

library L {
    function pick(uint256 self, bool x) internal pure returns (bool) {
        self;
        return x;
    }

    function pick(uint256 self, uint256 x) internal pure returns (uint256) {
        self;
        return x;
    }

    function pair(uint256 self, uint256 x, uint256 y) internal pure returns (uint256) {
        return self + x + y;
    }

    function onlySelf(uint256 self) internal pure returns (uint256) {
        return self;
    }

    function onlySelf(uint256 self, uint256 x) internal pure returns (uint256) {
        return self + x;
    }
}

contract C {
    using L for uint256;

    function f(uint256 x, bool y) public pure {
        uint256 z = 1;
        bool a = x.pick(y);
        uint256 b = x.pick(z);
        uint256 c = x.pair({x: 2, y: 3});
        uint256 d = x.onlySelf();
        uint256 e = x.onlySelf(z);
    }
}
