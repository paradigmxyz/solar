//@ revisions: petersburg istanbul
//@[petersburg] compile-flags: --evm-version petersburg
//@[istanbul] compile-flags: --evm-version istanbul
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_istanbul_on_petersburg.sol

contract C {
    function f() external view {
        assembly {
            pop(chainid())
            //~[petersburg]^ ERROR: Yul builtin `chainid` requires Istanbul-compatible EVM
            pop(selfbalance())
            //~[petersburg]^ ERROR: Yul builtin `selfbalance` requires Istanbul-compatible EVM
        }
    }
}
