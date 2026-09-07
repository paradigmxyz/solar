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

    // The expected element type seeds the literal, so an element can widen. solc types a literal
    // from its elements alone and rejects every one of these; see TYPECK-003 in
    // `docs/SOLC_DIVERGENCE.md`.
    function expectedElementType(bytes memory b) public pure {
        uint256[2] memory widened = [1, 2];
        int256[2] memory signed = [1, 2];
        bytes[2] memory bs = ["a", b];
        uint256[2][2] memory nested = [[1, 2], [3, 4]];
        widened;
        signed;
        bs;
        nested;
    }

    function bad() public pure {
        [1, "a"]; //~ ERROR: cannot infer array element type
    }
}
