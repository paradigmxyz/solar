//@ revisions: unlinked linked
//@[unlinked] compile-flags: -O none --emit=bin
//@[linked] compile-flags: --libraries Lib=0x1111111111111111111111111111111111111111 -Zdump=evm-ir-runtime
//@[linked] filecheck: --check-prefixes=COMMON,LINKED

// A `public`/`external` library function called from another contract is
// lowered to a DELEGATECALL when linked.

library Lib {
    // COMMON-LABEL: @module runtime
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

    // LINKED-LABEL: @module runtime
    // LINKED: push 0x3dd41ca6
    // LINKED: push 0xed2f0bb8
    // LINKED: mstore
    // LINKED: push 0x1111111111111111111111111111111111111111
    // LINKED: delegatecall
    // LINKED: push [[FAIL:bb[0-9]+]]
    // LINKED-NEXT: jumpi
    // LINKED: return
    // LINKED: [[FAIL]] [cold]:
    // LINKED: returndatacopy
    // LINKED: revert
    function inc(address k, uint256 by) external returns (uint256) {
        return Lib.bump(bal, k, by);
        //~[unlinked]^ ERROR: codegen requires a linked address for public library calls
    }
}
