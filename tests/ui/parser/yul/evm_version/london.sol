//@ revisions: london berlin
//@[london] compile-flags: --evm-version london
//@[berlin] compile-flags: --evm-version berlin
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/basefee_pre_london.sol

contract C {
    function identifier() external pure {
        assembly {
            let basefee
            //~[london]^ ERROR: cannot use builtin function name `basefee` as identifier name
            //~[berlin]^^ WARN: `basefee` will be promoted to a Yul reserved identifier
        }
    }

    function f() external view returns (uint256 fee) {
        assembly {
            fee := basefee()
            //~[berlin]^ ERROR: Yul builtin `basefee` requires London-compatible EVM
        }
    }
}
