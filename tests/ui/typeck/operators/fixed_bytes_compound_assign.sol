// Fixed bytes only take bitwise and shift compound assignments, like solc.

contract C {
    function accepted(bytes32 a, bytes32 b, bytes4 c, uint8 u) external pure {
        a &= b;
        a |= b;
        a ^= b;
        a &= c;
        a <<= u;
        a >>= u;
        a <<= 1;
        a >>= 1;
    }

    function arithmetic(bytes32 a, bytes32 b) external pure {
        a += b; //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `bytes32`
        a -= b; //~ ERROR: cannot apply builtin operator `-` to `bytes32` and `bytes32`
        a *= b; //~ ERROR: cannot apply builtin operator `*` to `bytes32` and `bytes32`
        a /= b; //~ ERROR: cannot apply builtin operator `/` to `bytes32` and `bytes32`
        a %= b; //~ ERROR: cannot apply builtin operator `%` to `bytes32` and `bytes32`
        a += 1; //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `int_literal[1]`
    }

    function mismatches(bytes32 a, bytes4 c, bytes32 b) external pure {
        c &= a; //~ ERROR: cannot apply builtin operator `&` to `bytes4` and `bytes32`
        c |= a; //~ ERROR: cannot apply builtin operator `|` to `bytes4` and `bytes32`
        a |= 1; //~ ERROR: cannot apply builtin operator `|` to `bytes32` and `int_literal[1]`
        a <<= b; //~ ERROR: cannot apply builtin operator `<<` to `bytes32` and `bytes32`
        a >>>= 1; //~ ERROR: cannot apply builtin operator `>>>` to `bytes32` and `int_literal[1]`
    }
}
