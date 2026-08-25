//@ revisions: istanbul petersburg
//@[istanbul] compile-flags: --evm-version istanbul
//@[petersburg] compile-flags: --evm-version petersburg
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_istanbul_on_petersburg.sol

contract C {
    function f() external view returns (uint256 id, uint256 selfBalance) {
        assembly {
            id := chainid()
            //~[petersburg]^ ERROR: Yul builtin `chainid` requires Istanbul-compatible EVM
            selfBalance := selfbalance()
            //~[petersburg]^ ERROR: Yul builtin `selfbalance` requires Istanbul-compatible EVM
        }
    }
}
