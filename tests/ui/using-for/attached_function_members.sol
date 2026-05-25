//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/assign_attached_library_function.sol
// ported-from: test/libsolidity/semanticTests/libraries/internal_library_function_attached_to_internal_function_type.sol
// ported-from: test/libsolidity/semanticTests/libraries/internal_library_function_attached_to_internal_function_type_named_selector.sol
// ported-from: test/libsolidity/semanticTests/libraries/internal_library_function_attached_to_external_function_type.sol

function inc(uint256 self) pure returns (uint256) {
    return self + 1;
}

function add(uint256 self, uint256 x) pure returns (uint256) {
    return self + x;
}

function doubleFn(function(uint256) internal pure returns (uint256) self, uint256 x)
    pure
    returns (uint256)
{
    return self(x) * 2;
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

    function callSelector(function(uint256) internal pure returns (uint256) self, uint256 x)
        internal
        pure
        returns (uint256)
    {
        return self(x) * 2;
    }

    function selector(function(uint256) internal pure returns (uint256) self, uint256 x)
        internal
        pure
        returns (uint256)
    {
        return self(x) * 2;
    }

    function callExternal(function(uint256) external pure returns (uint256) self, uint256 x)
        internal
        pure
        returns (uint256)
    {
        return self(x) * 2;
    }
}

library Mismatch {
    function nonpayableOnly(function(uint256) internal returns (uint256) self, uint256 x)
        internal
        returns (uint256)
    {
        return self(x) * 2;
    }
}

using {inc, add} for uint256;

contract C {
    using L for uint256;
    using L for function(uint256) internal pure returns (uint256);
    using L for function(uint256) external pure returns (uint256);
    using {doubleFn} for function(uint256) internal pure returns (uint256);

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

    function identity(uint256 x) internal pure returns (uint256) {
        return x;
    }

    function functionValue(uint256 x) public pure returns (uint256) {
        return identity.callSelector(x);
    }

    function functionValueSelector(uint256 x) public pure returns (uint256) {
        return identity.selector(x);
    }

    function functionValueFree(uint256 x) public pure returns (uint256) {
        return identity.doubleFn(x);
    }

    function externalIdentity(uint256 x) external pure returns (uint256) {
        return x;
    }

    function externalFunctionValue(uint256 x) public view returns (uint256) {
        return this.externalIdentity.callExternal(x);
    }
}

contract MismatchC {
    using Mismatch for function(uint256) internal returns (uint256);

    function identity(uint256 x) internal pure returns (uint256) {
        return x;
    }

    function bad(uint256 x) public pure returns (uint256) {
        return identity.nonpayableOnly(x); //~ ERROR: member `nonpayableOnly` not found
    }
}
