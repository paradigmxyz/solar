//@ revisions: homestead byzantium
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_byzantium_on_homestead.sol

contract C {
    function f() external view {
        assembly {
            pop(returndatasize())
            //~[homestead]^ ERROR: Yul builtin `returndatasize` requires Byzantium-compatible EVM
            returndatacopy(0, 0, 0)
            //~[homestead]^ ERROR: Yul builtin `returndatacopy` requires Byzantium-compatible EVM
            pop(staticcall(0, 0, 0, 0, 0, 0))
            //~[homestead]^ ERROR: Yul builtin `staticcall` requires Byzantium-compatible EVM

            let returndatasize
            //~^ ERROR: `returndatasize` is reserved for a Yul builtin

            let returndatacopy
            //~^ ERROR: `returndatacopy` is reserved for a Yul builtin

            let staticcall
            //~^ ERROR: `staticcall` is reserved for a Yul builtin

        }
    }
}
