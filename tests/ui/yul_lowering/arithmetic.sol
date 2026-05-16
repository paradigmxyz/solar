//@compile-flags: -Ztypeck

contract C {
    function f(uint256 a, uint256 b) public returns (uint256 x) {
        assembly {
            let c := add(a, b)
            c := sub(c, 1)
            c := mul(c, 2)
            c := div(c, 3)
            c := mod(c, 5)
            c := exp(c, 2)
            c := and(c, 0xff)
            c := or(c, 0x100)
            c := xor(c, 0x10)
            c := shl(1, c)
            c := shr(1, c)
            x := c
        }
    }
}
