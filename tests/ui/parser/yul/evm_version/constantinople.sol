//@ revisions: constantinople byzantium
//@[constantinople] compile-flags: --evm-version constantinople
//@[byzantium] compile-flags: --evm-version byzantium
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_constantinople_on_byzantium.sol

contract C {
    function f(uint256 value) external returns (uint256 result) {
        assembly {
            result := shl(1, value)
            //~[byzantium]^ ERROR: Yul builtin `shl` requires Constantinople-compatible EVM
            result := shr(1, value)
            //~[byzantium]^ ERROR: Yul builtin `shr` requires Constantinople-compatible EVM
            result := sar(1, value)
            //~[byzantium]^ ERROR: Yul builtin `sar` requires Constantinople-compatible EVM
            result := create2(0, 0, 0, 0)
            //~[byzantium]^ ERROR: Yul builtin `create2` requires Constantinople-compatible EVM
            result := extcodehash(address())
            //~[byzantium]^ ERROR: Yul builtin `extcodehash` requires Constantinople-compatible EVM
        }
    }
}
