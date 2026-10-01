//@ codegen-matrix: standard
//@ run-call-fail: shrink 3, 1 => Panic(50)
//@ run-call: shrink 3, 5 => 0x610000610000610000
// Assembly can shorten the output inside the loop, so a later index is checked against the length
// the object has then, not the capacity reserved before the loop.
contract ScaledCursorRawLength {
    function shrink(uint256 n, uint256 at) public pure returns (bytes memory out) {
        out = new bytes(n * 3);
        uint256 j;
        for (uint256 i; i < n; i++) {
            if (i == at) {
                assembly {
                    mstore(out, 1)
                }
            }
            out[j] = 0x61;
            j += 3;
        }
    }
}
