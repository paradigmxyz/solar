//@ check-pass
//@compile-flags: -Zcodegen --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=evm-ir-runtime

// With `--libraries Lib=0xADDR`, a call to a `public` library function is
// lowered as an ABI-encoded DELEGATECALL to the linked address (storage
// references travel as their slot; a failed call re-raises the callee's
// revert data) instead of inlining the library body — matching solc's library
// model and keeping the body out of the caller's runtime. The library's own
// runtime is unchanged. Runtime behavior is verified against solc's linked
// flow separately (including a two-level library-to-library chain).

library Lib {
    function bump(mapping(address => uint256) storage m, address k, uint256 by)
        public
        returns (uint256)
    {
        m[k] += by;
        return m[k];
    }
}

contract C {
    mapping(address => uint256) bal;

    function inc(address k, uint256 by) external returns (uint256) {
        return Lib.bump(bal, k, by);
    }
}
