//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_functions_inside_contract.sol

library L {
    function pick(uint256 self, uint8 x) internal pure returns (uint8) {
        self;
        return x;
    }

    function pick(uint256 self, uint16 x) internal pure returns (uint16) {
        self;
        return x;
    }
}

contract C {
    using L for uint256;

    function f(uint256 x) public pure {
        x.pick(1); //~ ERROR: member `pick` not unique
    }
}
