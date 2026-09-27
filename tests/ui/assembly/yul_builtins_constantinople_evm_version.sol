//@ revisions: byzantium constantinople
//@[byzantium] compile-flags: --evm-version byzantium
//@[constantinople] compile-flags: --evm-version constantinople
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_constantinople_on_byzantium.sol

contract C {
    function f() external {
        assembly {
            pop(shl(1, 1))
            //~[byzantium]^ ERROR: Yul builtin `shl` requires Constantinople-compatible EVM
            pop(shr(1, 1))
            //~[byzantium]^ ERROR: Yul builtin `shr` requires Constantinople-compatible EVM
            pop(sar(1, 1))
            //~[byzantium]^ ERROR: Yul builtin `sar` requires Constantinople-compatible EVM
            pop(create2(0, 0, 0, 0))
            //~[byzantium]^ ERROR: Yul builtin `create2` requires Constantinople-compatible EVM
            pop(extcodehash(0))
            //~[byzantium]^ ERROR: Yul builtin `extcodehash` requires Constantinople-compatible EVM
        }
    }
}
