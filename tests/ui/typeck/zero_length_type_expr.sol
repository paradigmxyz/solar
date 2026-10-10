// Array type expressions may have zero length; only array type names may not.

struct S {
    uint x;
}

contract C {
    function f(bytes memory d) public pure {
        uint[0];
        S[0];
        abi.decode(d, (uint[0]));
        abi.decode(d, (uint[0][2], uint[2][0], S[0], C[0]));

        uint[0] memory a; //~ ERROR: array length must be greater than zero
    }
}
