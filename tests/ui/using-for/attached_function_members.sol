//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/assign_attached_library_function.sol

function inc(uint256 self) pure returns (uint256) {
    return self + 1;
}

function add(uint256 self, uint256 x) pure returns (uint256) {
    return self + x;
}

library L {
    function foo(uint256 self, uint256 x) internal pure returns (uint256) {
        return self + x;
    }

    function selector(uint256 self) public pure returns (uint256) {
        return self;
    }

    function ext(uint256 self) external pure returns (uint256) {
        return self;
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

using {inc, add} for uint256;

contract C {
    using L for uint256;

    function bad(uint256 x) public pure {
        x.inc;
        x.selector;
        x.selector.selector;
        x.ext.selector;
        x.ext.address; //~ ERROR: member `address` not found
        x.pick; //~ ERROR: member `pick` not unique
        function(uint256) internal pure returns (uint256) ptr = x.inc; //~ ERROR: mismatched types
        function(uint256, uint256) internal pure returns (uint256) ptr2 = x.foo; //~ ERROR: mismatched types
        x.inc(1); //~ ERROR: wrong argument count for function call
        x.add(); //~ ERROR: wrong argument count for function call
    }
}
