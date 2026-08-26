//@ revisions: prague osaka
//@[prague] compile-flags: --evm-version prague
//@[osaka] compile-flags: --evm-version osaka
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clz_pre_osaka.sol

contract C {
    function f() external pure {
        assembly {
            pop(clz(0))
            //~[prague]^ ERROR: Yul builtin `clz` requires Osaka-compatible EVM
        }
    }
}
