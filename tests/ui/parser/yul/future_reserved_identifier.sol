// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clash_with_non_reserved_pure_yul_builtin.sol

contract C {
    function f() external pure {
        assembly {
            let memoryguard
            //~^ WARN: `memoryguard` will be promoted to a Yul reserved identifier
        }
    }
}
