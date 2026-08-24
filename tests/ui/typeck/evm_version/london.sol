//@ revisions: berlin london
//@[berlin] compile-flags: --evm-version berlin
//@[london] compile-flags: --evm-version london
// ported-from: test/libsolidity/syntaxTests/types/magic_block_basefee_error.sol

contract C {
    function basefee() public view returns (uint256) {
        return block.basefee;
        //~[berlin]^ ERROR: builtin `basefee` requires London-compatible EVM
    }
}
