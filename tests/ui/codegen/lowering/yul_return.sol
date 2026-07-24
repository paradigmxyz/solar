//@ check-pass
//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// The Yul `return(offset, size)` builtin halts execution and returns `size`
// bytes of memory (the `RETURN` opcode), like the `ret_data` terminator. It is
// used by delegatecall proxy fallbacks (e.g. OpenZeppelin `Proxy`). Runtime
// behavior is verified against solc 0.8.30 separately.

contract R {
    function echo(uint256 x) external pure returns (uint256) {
        assembly {
            mstore(0, x)
            return(0, 32)
        }
    }
}
