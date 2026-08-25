//@ revisions: osaka prague
//@[osaka] compile-flags: --evm-version osaka
//@[prague] compile-flags: --evm-version prague
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clz_reserved_osaka.sol
// ported-from: test/libsolidity/semanticTests/inlineAssembly/clz_pre_osaka.sol

contract C {
    function f() external pure returns (uint256 result) {
        assembly {
            function clz() -> value {
                //~[osaka]^ ERROR: cannot use builtin function name `clz` as identifier name
                //~[prague]^^ WARN: `clz` will be promoted to a Yul reserved identifier
                value := 1
            }
            result := clz()
        }
    }
}
