// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clash_with_reserved_non_builtin.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/clash_with_reserved_pure_yul_builtin.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/linkersymbol_function.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/reserved_identifiers.sol

contract C {
    function nonBuiltin() public pure {
        assembly {
            // NOTE: All EVM instruction names are reserved identifiers in Yul.
            // NOTE: We don't provide builtins corresponding to these instructions.
            function dup1(dup2) -> dup3 {} //~ ERROR: identifier `dup1` is reserved and cannot be used
            //~^ ERROR: identifier `dup2` is reserved and cannot be used
            //~| ERROR: identifier `dup3` is reserved and cannot be used
            let dup4 //~ ERROR: identifier `dup4` is reserved and cannot be used
        }
    }

    function pureYulBuiltin() public view {
        assembly {
            // NOTE: These are builtins but only in pure Yul, not inline assembly.
            // NOTE: Names of these builtins are also reserved identifiers.
            function loadimmutable(setimmutable) -> datasize {} //~ ERROR: identifier `loadimmutable` is reserved and cannot be used
            //~^ ERROR: identifier `setimmutable` is reserved and cannot be used
            //~| ERROR: identifier `datasize` is reserved and cannot be used
            let dataoffset //~ ERROR: identifier `dataoffset` is reserved and cannot be used
        }
    }

    function linkersymbolFunction() public pure {
        assembly {
            function linkersymbol(a) {} //~ ERROR: identifier `linkersymbol` is reserved and cannot be used

            linkersymbol("contract/library.sol:L")
        }
    }

    function reservedIdentifiers() public pure {
        assembly {
            let linkersymbol := 1 //~ ERROR: identifier `linkersymbol` is reserved and cannot be used
            let datacopy := 1 //~ ERROR: identifier `datacopy` is reserved and cannot be used
            let swap16 := 1 //~ ERROR: identifier `swap16` is reserved and cannot be used
        }
    }

    function instructions() public pure {
        assembly {
            let jump := 1 //~ ERROR: identifier `jump` is reserved and cannot be used
            let jumpi := 1 //~ ERROR: identifier `jumpi` is reserved and cannot be used
            let jumpdest := 1 //~ ERROR: identifier `jumpdest` is reserved and cannot be used
            let pc := 1 //~ ERROR: identifier `pc` is reserved and cannot be used
            let push0 := 1 //~ ERROR: identifier `push0` is reserved and cannot be used
            let push32 := 1 //~ ERROR: identifier `push32` is reserved and cannot be used
            let dup16 := 1 //~ ERROR: identifier `dup16` is reserved and cannot be used
            let swap1 := 1 //~ ERROR: identifier `swap1` is reserved and cannot be used
        }
    }

    function notReserved() public pure {
        assembly {
            let push33 := 1
            let push01 := 1
            let dup0 := 1
            let dup17 := 1
            let swap0 := 1
            let swap17 := 1
            let jumps := 1
        }
    }
}
