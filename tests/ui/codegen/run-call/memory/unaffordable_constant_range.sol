//@ compile-flags: -Ogas
//@ run-call: skip 1 => 3
//@ run-call: skip 0 => 3
//@ run-call-fail: touch 1 =>
// A constant memory range that ends beyond what any transaction can pay for runs out of gas when
// it executes, so it does not move the heap floor, and a path that skips it still allocates.

contract UnaffordableConstantRange {
    function skip(uint256 c) external pure returns (uint256) {
        if (c == 2) {
            assembly {
                revert(0x80, 0x7000000000000000)
            }
        }
        uint256[2] memory a = [c, c + 1];
        return a[0] + a[1] + 2 - c * 2;
    }

    function touch(uint256 c) external pure returns (uint256) {
        assembly {
            if c {
                revert(0x80, 0x7000000000000000)
            }
        }
        uint256[2] memory a = [c, c + 1];
        return a[0] + a[1];
    }
}
