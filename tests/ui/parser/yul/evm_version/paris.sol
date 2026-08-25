//@ revisions: paris london
//@[paris] compile-flags: --evm-version paris
//@[london] compile-flags: --evm-version london
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_allowed_function_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_disallowed_function_post_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_disallowed_function_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_reserved_post_paris.sol

contract C {
    function identifiers() external pure {
        assembly {
            let prevrandao
            //~[paris]^ ERROR: cannot use builtin function name `prevrandao` as identifier name
            //~[london]^^ WARN: `prevrandao` will be promoted to a Yul reserved identifier
            let difficulty
            //~[london]^ ERROR: cannot use builtin function name `difficulty` as identifier name
            //~[paris]^^ ERROR: identifier `difficulty` is reserved and cannot be used
        }
    }
}
