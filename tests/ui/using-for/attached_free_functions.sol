//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/using/free_functions_individual.sol
// ported-from: test/libsolidity/semanticTests/using/free_function_multi.sol
// ported-from: test/libsolidity/semanticTests/using/library_functions_inside_contract.sol

function inc(uint256 self) pure returns (uint256) {
    return self + 1;
}

function add(uint256 self, uint256 x) pure returns (uint256) {
    return self + x;
}

function id(uint256 self) pure returns (uint256) {
    return self;
}

function zero(uint256) pure returns (uint256) {
    return 0;
}

library L {
    function externalFunction(uint256 self) external pure returns (uint256) {
        return self;
    }

    function publicFunction(uint256 self) public pure returns (uint256) {
        return self * 2;
    }

    function internalFunction(uint256 self) internal pure returns (uint256) {
        return self * 3;
    }

    function pick(uint256 self, bool x) internal pure returns (bool) {
        self;
        return x;
    }

    function pick(uint256 self, uint256 x) internal pure returns (uint256) {
        self;
        return x;
    }
}

using {inc, add, zero} for uint256;

contract C {
    using {id} for uint256;
    using L for uint256;

    function ok(uint256 x, bool b) public pure {
        uint256 a = x.inc();
        uint256 c = x.add(1);
        bool d = x.pick(b);
        uint256 e = x.pick(1);
        uint256 f = x.id();
        uint256 g = x.zero();
        uint256 h = x.externalFunction();
        uint256 i = x.publicFunction();
        uint256 j = x.internalFunction();
        a; c; d; e; f; g; h; i; j;
    }
}
