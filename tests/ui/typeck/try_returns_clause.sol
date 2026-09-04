// ported-from: test/libsolidity/syntaxTests/tryCatch/invalid_returns.sol
// ported-from: test/libsolidity/syntaxTests/tryCatch/returns_mismatch.sol

// A `returns` clause has to repeat the callee's return types exactly. The decoder writes the
// call's values straight into the clause's variables, so neither an implicit conversion nor a
// data location other than `memory` is accepted.

interface I {
    function none() external;
    function one() external returns (uint256);
    function two() external returns (uint8, uint256);
    function arr() external returns (uint256[] memory);
    function mixed() external returns (uint256, bytes memory);
}

contract D {}

library L {
    function slot(uint256[] storage s) external returns (uint256[] storage) {
        return s;
    }

    // A clause parameter is declared in the clause's own scope, so a library function does not
    // widen its locations the way it widens its own parameters'.
    function inLibrary(address a) internal {
        try I(a).arr() returns (uint256[] storage r) {
            //~^ ERROR: invalid data location `storage`
            r;
        } catch {}
    }
}

// A constructor's parameters may be `storage` pointers; a clause's may not.
contract Constructing {
    constructor(address a) {
        try I(a).arr() returns (uint256[] storage r) {
            //~^ ERROR: invalid data location `storage`
            r;
        } catch {}
    }
}

contract Mismatch {
    function f() public returns (uint8, uint256) {
        // Implicitly convertible, but not exactly the same type.
        try this.f() returns (uint256, int256 x) {
            //~^ ERROR: mismatched types
            //~| ERROR: mismatched types
            x;
        } catch {}
    }

    function g() public returns (uint256, uint256) {
        try this.g() returns (uint256 a) {
            //~^ ERROR: function returns 2 values, but the `returns` clause has 1 variable
            a = 1;
        } catch {}
    }
}

contract C {
    uint256[] s;

    // A count mismatch in either direction, including against a callee with no return values.
    function count(address a) external {
        try I(a).one() returns (uint256 x, uint256 y) {
            //~^ ERROR: function returns 1 value, but the `returns` clause has 2 variables
            x;
            y;
        } catch {}
        try I(a).none() returns (uint256 x) {
            //~^ ERROR: function returns 0 values, but the `returns` clause has 1 variable
            x;
        } catch {}
    }

    // A count mismatch does not stop the pairs that are there from being compared.
    function countAndType(address a) external {
        try I(a).two() returns (bool b) {
            //~^ ERROR: function returns 2 values, but the `returns` clause has 1 variable
            //~| ERROR: mismatched types
            b;
        } catch {}
    }

    function type_(address a) external {
        try I(a).one() returns (bool b) {
            //~^ ERROR: mismatched types
            b;
        } catch {}
    }

    // The data location is part of the type, so a memory binding of a storage pointer is a
    // mismatch. A library function returning one cannot be bound at all: every other location
    // is rejected outright.
    function location() external {
        try L.slot(s) returns (uint256[] memory r) {
            //~^ ERROR: mismatched types
            r;
        } catch {}
        try L.slot(s) returns (uint256[] storage r) {
            //~^ ERROR: invalid data location `storage`
            r;
        } catch {}
    }

    // A location on a type that takes none is just as illegal, and the declaration is the only
    // error: the variable's rewritten type is not what was written, so comparing it would
    // report a second, invented mismatch.
    function nonReferenceLocation(address a) external {
        try I(a).one() returns (bool memory b) {
            //~^ ERROR: data location can only be specified for array, struct or mapping types
            b;
        } catch {}
        try I(a).one() returns (uint256 calldata x) {
            //~^ ERROR: data location can only be specified for array, struct or mapping types
            x;
        } catch {}
        try I(a).one() returns (bool storage c) {
            //~^ ERROR: data location can only be specified for array, struct or mapping types
            c;
        } catch {}
    }

    // A creation call returns the new contract, not its address.
    function creation() external {
        try new D() returns (address b) {
            //~^ ERROR: mismatched types
            b;
        } catch {}
        try new D() returns (D d, uint256 x) {
            //~^ ERROR: function returns 1 value, but the `returns` clause has 2 variables
            d;
            x;
        } catch {}
    }

    // An external function pointer callee carries the same return types.
    function pointer(address a) external {
        function() external returns (uint256) p = I(a).one;
        try p() returns (bool b) {
            //~^ ERROR: mismatched types
            b;
        } catch {}
    }

    // Only an external, delegate, or creation call decodes return values into the clause. Every
    // other callee is rejected outright and its clause compared to nothing: a builtin's return
    // values are not even always a type. See `try_catch_clause_checks.sol`.
    function otherCallees(bytes memory d) external {
        try internal_() returns (bool b) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            b;
        } catch {}
        try abi.decode(d, (uint256)) returns (uint256 x) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            x;
        } catch {}
        try gasleft() returns (bool b) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            b;
        } catch {}
        try address(this).call("") returns (uint256 ok) {
            //~^ ERROR: `try` can only be used with external function calls and contract creation calls
            ok;
        } catch {}
    }

    function internal_() internal pure returns (uint256) {
        return 1;
    }

    // Nor does an internal function, whose own parameters may be `storage` pointers.
    function inInternal(address a) internal {
        try I(a).arr() returns (uint256[] storage r) {
            //~^ ERROR: invalid data location `storage`
            r;
        } catch {}
    }

    // Valid companions: the same types, a creation call, no `returns` clause at all, and a
    // mixed static and dynamic return.
    function valid(address a) external {
        try I(a).two() returns (uint8 x, uint256 y) {
            x;
            y;
        } catch {}
        try I(a).arr() returns (uint256[] memory r) {
            r;
        } catch {}
        try I(a).mixed() returns (uint256 x, bytes memory b) {
            x;
            b;
        } catch {}
        try new D() returns (D d) {
            d;
        } catch {}
        try I(a).two() {} catch {}
        try I(a).none() {} catch {}
    }
}
