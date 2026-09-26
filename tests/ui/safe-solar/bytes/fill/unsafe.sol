//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: blank 0x0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489ae, 3, 33 => 0x0b30552a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a3f6489ae
//@ run-call: blank 0x5555555555555555, 8, 0 => 0x5555555555555555
//@ run-call: blank 0x5555555555555555, 6, 4 => 0x5555555555552a2a

// The same fill by hand: whole words while a word remains, then a masked
// partial word. On valid input it agrees. Filling four bytes at offset six of
// an eight-byte buffer does not fail: two land inside, two overrun.
// CHECK-LABEL: fn @blank
// CHECK: mstore
contract Unsafe {
    function blank(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes memory) {
        assembly ("memory-safe") {
            let p := add(add(b, 0x20), offset)
            let end := add(p, count)
            let pattern := mul(0x2a, div(not(0), 255))
            for {} iszero(lt(sub(end, p), 32)) { p := add(p, 32) } { mstore(p, pattern) }
            let rest := sub(end, p)
            if rest {
                let keep := shr(mul(rest, 8), not(0))
                mstore(p, or(and(mload(p), keep), and(pattern, not(keep))))
            }
        }
        return b;
    }
}
