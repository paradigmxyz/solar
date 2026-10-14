contract C {
    bytes4 s;
    uint256 constant I = 2;

    function f(bytes2 b, uint256 i) public view {
        b[0];
        b[1];
        b[i];
        b[I];
        b[2]; //~ ERROR: out of bounds array access
        b[1 + 1]; //~ ERROR: out of bounds array access
        b[2 ** 256 - 1]; //~ ERROR: out of bounds array access
        s[3];
        s[4]; //~ ERROR: out of bounds array access
        bytes32(0)[31];
        bytes32(0)[64]; //~ ERROR: out of bounds array access
        bytes1(0)[0x01]; //~ ERROR: out of bounds array access
    }
}
