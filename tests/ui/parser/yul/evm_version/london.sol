//@ revisions: london berlin
//@[london] compile-flags: --evm-version london
//@[berlin] compile-flags: --evm-version berlin
contract C {
    function f() external view returns (uint256 fee) {
        assembly {
            fee := basefee()
            //~[berlin]^ ERROR: Yul builtin `basefee` requires London-compatible EVM
        }
    }
}
