//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// A `public`/`external` library function called from another contract is
// compiled by solc into the library's own runtime and reached via delegatecall
// with link-time address substitution. A library has no storage of its own and
// runs in the caller's storage/`msg` context, so inlining the function (or
// calling an internal-frame copy) yields the same result without a separately
// deployed and linked library. Used by aave-v3-core FlashLoanLogic, which calls
// the `public` `BorrowLogic.executeBorrow`.
//
// Runtime results (return value, mutated storage, and `msg.sender` use) are
// verified equal to solc 0.8.30 (with real library linking) separately.

library Lib {
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

    function inc(address k, uint256 by) external returns (uint256) {
        return Lib.bump(bal, k, by);
    }
}
