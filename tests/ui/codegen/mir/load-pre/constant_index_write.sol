//@ codegen-matrix: standard
//@ run-call: f true, true, 0x11, 0x22 => 0x22
//@ run-call: f true, false, 0x11, 0x22 => 0x22
//@ run-call: f false, true, 0x11, 0x22 => 0x11
//@ run-call: f false, false, 0x11, 0x22 => 0x11

// A constant-index `bytes` read spans past the end of the 64-byte allocation, so alias analysis
// names it `UnboundedAllocation`, while the constant-index write that fits is named `Allocation`.
// The write must still kill the read's key.
contract ConstantIndexWrite {
    function f(bool c0, bool c1, bytes1 v, bytes1 w) external pure returns (uint256 r) {
        bytes memory b = new bytes(32);
        b[5] = v;
        uint256 x = uint8(b[5]);
        uint256 y;
        if (c0) {
            b[5] = w;
            y = x + 1;
        } else {
            y = x + 2;
        }
        if (c1) {
            y = y + 10;
        } else {
            y = y + 20;
        }
        r = uint8(b[5]);
        if (y == type(uint256).max) {
            r = y;
        }
    }
}
