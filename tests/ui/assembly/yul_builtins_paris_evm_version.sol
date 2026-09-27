//@ revisions: london paris
//@[london] compile-flags: --evm-version london
//@[paris] compile-flags: --evm-version paris
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_nobuitin_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_nobuiltin_post_paris.sol

contract C {
    function f() external {
        assembly {
            pop(prevrandao())
            //~[london]^ ERROR: Yul builtin `prevrandao` requires Paris-compatible EVM
            pop(difficulty())
            //~[paris]^ ERROR: Yul builtin `difficulty` is unavailable for Paris-compatible EVM
        }
    }
}
