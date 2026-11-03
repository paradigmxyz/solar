//@ codegen-matrix: standard
//@ run-call: step 0x8000000000000000, 0x10000000000000000 => 0x16a09e667f3bcc909
//@ run-call: step 0x7fffffffffffffff, 0x10000000000000000 => 0x10000000000000000
//@ run-call: step 0x8000000000000000, 0 => 0
//@ run-call: step 0, 5 => 5
//@ run-call: two 0xc000000000000000, 0x10000000000000000 => 0x1ae89f995ad3ad5e8
//@ run-call: two 0x4000000000000000, 0x10000000000000000 => 0x1306fe0a31b7152de

// A branch whose step multiplies the running value only when a bit is set: the selected form
// would run the multiplication on every step, so the branches stay.
contract Exp2Steps {
    function step(uint256 x, uint256 result) external pure returns (uint256) {
        if (x & 0x8000000000000000 > 0) {
            result = (result * 0x16A09E667F3BCC909) >> 64;
        }
        return result;
    }

    function two(uint256 x, uint256 result) external pure returns (uint256) {
        if (x & 0x8000000000000000 > 0) {
            result = (result * 0x16A09E667F3BCC909) >> 64;
        }
        if (x & 0x4000000000000000 > 0) {
            result = (result * 0x1306FE0A31B7152DE) >> 64;
        }
        return result;
    }
}
