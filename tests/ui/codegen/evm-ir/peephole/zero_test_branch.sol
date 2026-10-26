//@ codegen-matrix: standard
//@ run-call: scale 0 => 0x1000000000000000000
//@ run-call: scale 1 => 0x1172b83c7d517adce00
//@ run-call: scale 0x100 => 0x1306fe0a31b7152df00
//@ run-call: scale 0x10000 => 0x10b5586cf9890f62a00
//@ run-call: scale 0x10000000 => 0x1000000000000000000
//@ run-call: scale 0xffffffff => 0x15ab07dd48542958db6
//@ run-call: ok 0 => true
//@ run-call: ok 1 => false

contract ZeroTestBranch {
    // Each masked nonzero test guards a body too large to if-convert under `-Osize`, so the
    // branch skips it when the masked value is zero.
    function scale(uint256 x) public pure returns (uint256 r) {
        r = 0x1000000000000000000;
        if (x & 0xff00 > 0) r = (r * 0x1306fe0a31b7152df) >> 64;
        if (x & 0x00ff > 0) r = (r * 0x1172b83c7d517adce) >> 64;
        if (x & 0x0f0f0000 > 0) r = (r * 0x10b5586cf9890f62a) >> 64;
    }

    function ok(uint256 x) public pure returns (bool) {
        return !(x > 0);
    }
}
