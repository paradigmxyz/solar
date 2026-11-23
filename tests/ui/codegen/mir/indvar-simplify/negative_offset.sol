//@ codegen-matrix: standard
//@ run-call: sum 0 => 0
//@ run-call: sum 8 => 0
//@ run-call: sum 9 => 1
//@ run-call: sum 10 => 3
// The scan reads element `i - 8` of a heap array, an address below the counter's by a constant.
// For a bound below the start the loop runs no iteration, but the pointer's end at that bound
// lies below the array, so it must not replace the counter's `lt` test, where it would wrap.
contract NegativeOffset {
    function sum(uint256 n) public pure returns (uint256 s) {
        uint256[] memory a = new uint256[](4);
        a[0] = 1;
        a[1] = 2;
        assembly {
            let bound := and(n, 0xff)
            for { let i := 8 } lt(i, bound) { i := add(i, 1) } {
                s := add(s, mload(add(a, sub(shl(5, i), 0xe0))))
            }
        }
    }
}
