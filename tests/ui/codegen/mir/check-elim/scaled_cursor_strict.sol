//@ codegen-matrix: standard
//@ run-call-fail: past 0x61 => Panic(50)
//@ run-call-fail: past 0x616263 => Panic(50)
//@ run-call: within 0x616263 => 0x610061620062630063
// Each input byte reserves three output bytes, so the cursor plus three reaches the capacity:
// an index there is out of bounds, while the first and last reserved bytes are in bounds.
contract ScaledCursor {
    function past(bytes memory input) public pure returns (bytes memory out) {
        out = new bytes(input.length * 3);
        uint256 j;
        for (uint256 i; i < input.length; i++) {
            out[j + 3] = input[i];
            j += 3;
        }
    }

    function within(bytes memory input) public pure returns (bytes memory out) {
        out = new bytes(input.length * 3);
        uint256 j;
        for (uint256 i; i < input.length; i++) {
            out[j] = input[i];
            out[j + 2] = input[i];
            j += 3;
        }
    }
}
