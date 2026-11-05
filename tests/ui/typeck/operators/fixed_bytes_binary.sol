// Fixed bytes compare, combine bitwise, and shift by unsigned amounts, like solc. A literal on the
// left only combines with integers.

contract C {
    function accepted(bytes32 a, bytes32 b, bytes4 c, uint8 u) external pure {
        a & b;
        a | b;
        a ^ b;
        a & c;
        c | a;
        a << u;
        a >> u;
        a << 1;
        a >> 1;
        a < b;
        a <= b;
        a > b;
        a >= b;
        a == b;
        a != b;
        a == c;
        c < a;
        a == 0;
        ~a;
    }

    function arithmetic(bytes32 a, bytes32 b, bytes4 c) external pure returns (bytes32, bytes32) {
        a + b; //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `bytes32`
        a - b; //~ ERROR: cannot apply builtin operator `-` to `bytes32` and `bytes32`
        a * b; //~ ERROR: cannot apply builtin operator `*` to `bytes32` and `bytes32`
        a / b; //~ ERROR: cannot apply builtin operator `/` to `bytes32` and `bytes32`
        a % b; //~ ERROR: cannot apply builtin operator `%` to `bytes32` and `bytes32`
        a ** b; //~ ERROR: cannot apply builtin operator `**` to `bytes32` and `bytes32`
        a + c; //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `bytes4`
        a + 1; //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `int_literal[1]`
        1 + a; //~ ERROR: cannot apply builtin operator `+` to `int_literal[1]` and `bytes32`
        0 == a; //~ ERROR: cannot apply builtin operator `==` to `int_literal[1]` and `bytes32`
        return (a + b, a * b); //~ ERROR: cannot apply builtin operator `+` to `bytes32` and `bytes32`
        //~^ ERROR: cannot apply builtin operator `*` to `bytes32` and `bytes32`
    }

    function logical(bytes32 a, bytes32 b) external pure {
        a && b; //~ ERROR: cannot apply builtin operator `&&` to `bytes32` and `bytes32`
        a || b; //~ ERROR: cannot apply builtin operator `||` to `bytes32` and `bytes32`
    }

    function shifts(bytes32 a, bytes32 b, int8 s) external pure {
        a << b; //~ ERROR: cannot apply builtin operator `<<` to `bytes32` and `bytes32`
        a >> s; //~ ERROR: cannot apply builtin operator `>>` to `bytes32` and `int8`
        a >>> 1; //~ ERROR: cannot apply builtin operator `>>>` to `bytes32` and `int_literal[1]`
    }

    function literals(bytes1 x) external pure {
        x & 0x01;
        x == 0x01;
        x + 0x01; //~ ERROR: cannot apply builtin operator `+` to `bytes1` and `int_literal[1]`
    }
}
