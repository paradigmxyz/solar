//@ revisions: london paris
//@[london] compile-flags: --evm-version london
//@[paris] compile-flags: --evm-version paris
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_nobuitin_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_nobuiltin_post_paris.sol

contract C {
    function f() external {
        assembly {
            pop(prevrandao())
            //~[london]^ ERROR: Yul builtin `prevrandao` requires Paris-compatible EVM
            pop(difficulty())
            //~[paris]^ ERROR: Yul builtin `difficulty` is unavailable for Paris-compatible EVM
        }
    }

    function read() external pure returns (uint difficulty) {
        assembly { difficulty := add(difficulty, 1) }
        //~[london]^ ERROR: expected identifier, found Yul EVM builtin keyword `difficulty`
        //~[london]| ERROR: builtin function `difficulty` must be called
    }

    function identifiers() external pure {
        assembly {
            let difficulty := 1 //~ ERROR: expected identifier, found Yul EVM builtin keyword `difficulty`
        }
        assembly {
            function difficulty() {} //~ ERROR: expected identifier, found Yul EVM builtin keyword `difficulty`
        }
        assembly {
            function helper(difficulty) {} //~ ERROR: expected identifier, found Yul EVM builtin keyword `difficulty`
        }
        assembly {
            function helper() -> difficulty {} //~ ERROR: expected identifier, found Yul EVM builtin keyword `difficulty`
        }
    }
}
