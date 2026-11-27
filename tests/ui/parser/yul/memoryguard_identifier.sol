// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clash_with_non_reserved_pure_yul_builtin.sol

contract C {
    function f() public pure {
        // NOTE: memoryguard is a builtin but only in pure Yul, not inline assembly.
        // NOTE: memoryguard is not a reserved identifier.
        assembly { function memoryguard() {} }
        //~^ WARN: `memoryguard` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        assembly { function f(memoryguard) {} }
        //~^ WARN: `memoryguard` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        assembly { function f() -> memoryguard {} }
        //~^ WARN: `memoryguard` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
        assembly { let memoryguard }
        //~^ WARN: `memoryguard` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
    }
}
