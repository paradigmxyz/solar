//@ revisions: london paris parse
//@[london] compile-flags: --evm-version london
//@[paris] compile-flags: --evm-version paris
//@[parse] compile-flags: --stop-after parsing --evm-version london
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
        assembly { difficulty := 1 }
        //~[london]^ ERROR: builtin function `difficulty` must be called
    }

    function reserved() external pure {
        assembly {
            let difficulty := 1
            //~[london,paris]^ ERROR: `difficulty` is reserved for a Yul builtin
        }
        assembly {
            function difficulty() {}
            //~[london,paris]^ ERROR: `difficulty` is reserved for a Yul builtin
        }
        assembly {
            function helper(difficulty) {}
            //~[london,paris]^ ERROR: `difficulty` is reserved for a Yul builtin
        }
        assembly {
            function helper() -> difficulty {}
            //~[london,paris]^ ERROR: `difficulty` is reserved for a Yul builtin
        }
    }
}
