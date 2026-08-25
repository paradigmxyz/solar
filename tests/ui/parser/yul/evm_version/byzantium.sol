//@ revisions: byzantium homestead
//@[byzantium] compile-flags: --evm-version byzantium
//@[homestead] compile-flags: --evm-version homestead
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/evm_byzantium_on_homestead.sol

contract C {
    function f() external view returns (uint256 size, bool success) {
        assembly {
            size := returndatasize()
            //~[homestead]^ ERROR: Yul builtin `returndatasize` requires Byzantium-compatible EVM
            returndatacopy(0, 0, size)
            //~[homestead]^ ERROR: Yul builtin `returndatacopy` requires Byzantium-compatible EVM
            success := staticcall(0, 0, 0, 0, 0, 0)
            //~[homestead]^ ERROR: Yul builtin `staticcall` requires Byzantium-compatible EVM
        }
    }
}
