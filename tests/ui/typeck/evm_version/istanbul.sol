//@ revisions: petersburg istanbul
//@[petersburg] compile-flags: --evm-version petersburg
//@[istanbul] compile-flags: --evm-version istanbul
// ported-from: test/libsolidity/syntaxTests/types/magic_block.sol

contract C {
    function chainid() public view returns (uint256) {
        return block.chainid;
        //~[petersburg]^ ERROR: builtin `chainid` requires Istanbul-compatible EVM
    }
}
