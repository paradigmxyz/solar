contract C {
    uint[2] s;
    uint[2][3] s2;
    uint constant K = 5;

    function f(uint[3] calldata cd, uint[] calldata dyn) external {
        uint[4] memory m;
        bytes4 b;
        bytes memory bs;

        s[1];
        s[2]; //~ ERROR: out of bounds array access
        s2[2][1];
        s2[3][0]; //~ ERROR: out of bounds array access
        s2[1][2]; //~ ERROR: out of bounds array access
        cd[2];
        cd[3]; //~ ERROR: out of bounds array access
        m[1 + 3]; //~ ERROR: out of bounds array access
        [1, 2][2]; //~ ERROR: out of bounds array access
        delete s[2]; //~ ERROR: out of bounds array access
        b[3];
        b[4]; //~ ERROR: out of bounds array access
        b[2 ** 256 - 1]; //~ ERROR: out of bounds array access
        bytes32(0)[31];
        bytes32(0)[64]; //~ ERROR: out of bounds array access
        bytes1(0)[0x01]; //~ ERROR: out of bounds array access

        // Literal quotients are exact.
        m[8 / 2]; //~ ERROR: out of bounds array access
        s[4 - (3 / 2) * 2];
        b[6 - (3 / 2) * 2];
        s[5 - (3 / 2) * 2]; //~ ERROR: out of bounds array access

        // Only literal indexes are checked.
        m[K];
        m[uint(4)];
        m[true ? 4 : 5];
        dyn[100];
        bs[100];
    }
}
