// Inline array literals are rejected when no element type, seeded by the destination or not,
// takes every element without loss.

contract C {
    function id(int256[3] memory a) internal pure returns (int256[3] memory) {
        return a;
    }

    function unseeded() public pure {
        [1, -1]; //~ ERROR: cannot infer array element type
        [uint8(1), int8(-1)]; //~ ERROR: cannot infer array element type
        [address(0), 0]; //~ ERROR: cannot infer array element type
        abi.encode([100, 0, int256(-100)]); //~ ERROR: cannot infer array element type
        id([100, 0, int256(-100)]); //~ ERROR: cannot infer array element type
    }

    function seeded() public pure {
        uint256[2] memory a = [1, -1]; //~ ERROR: cannot infer array element type
        int256[2] memory b = [uint8(1), int8(-1)]; //~ ERROR: cannot infer array element type
        int256[2] memory c = [uint256(1), -1]; //~ ERROR: cannot infer array element type
        address[2] memory d = [address(0), 0]; //~ ERROR: cannot infer array element type
        uint8[2] memory e = [uint8(1), 300]; //~ ERROR: mismatched types
        int256[2][2] memory f = [[1, -1], [uint256(1), 0]]; //~ ERROR: cannot infer array element type
        a;
        b;
        c;
        d;
        e;
        f;
    }
}
