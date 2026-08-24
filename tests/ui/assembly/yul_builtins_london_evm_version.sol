//@ revisions: berlin london
//@[berlin] compile-flags: --evm-version berlin
//@[london] compile-flags: --evm-version london
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/basefee_pre_london.sol

contract C {
    function f() external view {
        assembly {
            pop(basefee())
            //~[berlin]^ ERROR: Yul builtin `basefee` requires London-compatible EVM
        }
    }
}
