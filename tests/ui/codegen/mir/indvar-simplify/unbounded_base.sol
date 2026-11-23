//@ codegen-matrix: standard
//@ run-call: f 115792089237316195423570985008687907853269984665640564039457584007913129639803, 20, false => 20
//@ run-call: f 0, 20, false => 20
//@ run-call: f 0, 0, false => 0
// The loop's address is the free memory pointer plus a caller's word, which may lie anywhere.
// Near the top of the word, the pointer's end at the bound wraps below its start, so a `ptr <
// end` test would skip a loop that, without loads, runs to its bound.
contract UnboundedBase {
    function f(uint256 off, uint256 n, bool load) external pure returns (uint256 s) {
        assembly {
            let p := add(mload(64), off)
            n := and(n, 0xffff)
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                switch load
                case 0 { s := add(s, 1) }
                default { s := add(s, mload(add(p, mul(i, 32)))) }
            }
        }
    }
}
