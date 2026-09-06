//@ revisions: unlinked linked
//@[unlinked] compile-flags: -O none --emit=bin
//@[linked] compile-flags: --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=evm-ir-runtime
//@[linked] filecheck: --check-prefixes=COMMON,LINKED

// A `public`/`external` library function called from another contract is
// lowered to a DELEGATECALL.

library Lib {
    // COMMON-LABEL: @module Lib_runtime
    // COMMON: push 0xed2f0bb8
    // COMMON: keccak256
    // COMMON: sload
    // COMMON: sstore
    // COMMON: caller
    // COMMON: return
    function bump(mapping(address => uint256) storage m, address k, uint256 by)
        public
        returns (uint256)
    {
        m[k] += by;
        return m[k] + uint256(uint160(msg.sender) & 0xff);
    }
}

contract C {
    mapping(address => uint256) bal;

    // LINKED-LABEL: @module C_runtime
    // LINKED: push 0x3dd41ca6
    // LINKED: push 0x1da5e177
    // LINKED-NEXT: push 227
    // LINKED-NEXT: shl
    // LINKED: mstore
    // LINKED: push 0x1111111111111111111111111111111111111111
    // LINKED: delegatecall
    // LINKED-NEXT: jumpi [[SUCCESS:bb[0-9]+]], [[FAIL:bb[0-9]+]]
    // LINKED-NEXT: [[FAIL]] [cold]:
    // LINKED-NEXT: returndatasize
    // LINKED-NEXT: push 0
    // LINKED-NEXT: push 0
    // LINKED-NEXT: returndatacopy
    // LINKED-NEXT: returndatasize
    // LINKED-NEXT: push 0
    // LINKED-NEXT: revert
    // LINKED-NEXT: [[SUCCESS]]:
    // LINKED: return
    function inc(address k, uint256 by) external returns (uint256) {
        return Lib.bump(bal, k, by);
    }
}
