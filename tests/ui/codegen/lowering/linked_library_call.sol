//@compile-flags: -Zcodegen --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=evm-ir-runtime
//@ filecheck:

// With `--libraries Lib=0xADDR`, a call to a `public` library function is
// lowered as an ABI-encoded DELEGATECALL to the linked address (storage
// references travel as their slot; a failed call re-raises the callee's
// revert data) instead of inlining the library body — matching solc's library
// model and keeping the body out of the caller's runtime. The library's own
// runtime is unchanged. Runtime behavior is verified against solc's linked
// flow separately (including a two-level library-to-library chain).

library Lib {
    // CHECK-LABEL: @module runtime
    // CHECK: push 0xed2f0bb8
    // CHECK: keccak256
    // CHECK: sload
    // CHECK: sstore
    // CHECK: return
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

    // CHECK-LABEL: @module runtime
    // CHECK: push 0x3dd41ca6
    // CHECK: push 0xed2f0bb8
    // CHECK: mstore
    // CHECK: push 0x1111111111111111111111111111111111111111
    // CHECK: delegatecall
    // CHECK: push [[FAIL:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: return
    // CHECK: [[FAIL]] [cold]:
    // CHECK: returndatacopy
    // CHECK: revert
    function inc(address k, uint256 by) external returns (uint256) {
        return Lib.bump(bal, k, by);
    }
}
