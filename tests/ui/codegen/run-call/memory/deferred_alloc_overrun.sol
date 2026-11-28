//@ codegen-matrix: standard
//@[mir] run-call: g => 0x2222
//@[gas] run-call: g => 0x1111
//@[size] run-call: g => 0x1111

// With optimization, `q` never escapes, so it sits in a static slot rather than right after `p`,
// and the store past the end of `p` does not reach it. Under solc, and without optimization, `q`
// starts at `p + 32` and `g` returns 0x2222.
// See CODEGEN-037 in docs/SOLC_DIVERGENCE.md.
contract DeferredAllocOverrun {
    function g() external pure returns (uint256 r) {
        uint256[1] memory p = [uint256(1)];
        uint256[1] memory q = [uint256(2)];
        assembly {
            mstore(add(p, 32), 0x1111)
            mstore(q, 0x2222)
            r := mload(add(p, 32))
        }
    }
}
