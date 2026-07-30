// Inline arrays infer reference element types: string literals mobilize to
// `string memory`, and struct, bytes, and nested-array elements work the
// same. The inferred element type is nameable despite carrying a location.

contract C {
    struct S {
        uint256 a;
    }

    function ok(string memory v, bytes memory b) public pure {
        string[3] memory strs = ["lit", v, "x"];
        bytes[2] memory bs = [b, bytes("y")];
        S[2] memory ss = [S(1), S(2)];
        uint256[2][2] memory nested = [[uint256(1), 2], [uint256(3), 4]];
        strs;
        bs;
        ss;
        nested;
    }

    function bad() public pure {
        [1, "a"]; //~ ERROR: cannot infer array element type
    }
}
