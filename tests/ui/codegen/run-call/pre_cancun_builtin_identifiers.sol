//@ revisions: paris shanghai
//@[paris] compile-flags: --evm-version paris
//@[shanghai] compile-flags: --evm-version shanghai
//@[paris] run-call: PreCancunBuiltinIdentifiers::variables() => 31
//@[paris] run-call: PreCancunBuiltinIdentifiers::functions() => 31
//@[shanghai] run-call: PreCancunBuiltinIdentifiers::variables() => 31
//@[shanghai] run-call: PreCancunBuiltinIdentifiers::functions() => 31
// ported-from: test/libsolidity/semanticTests/inlineAssembly/blobbasefee_shanghai_function.sol
// ported-from: test/libsolidity/semanticTests/inlineAssembly/blobhash_pre_cancun.sol
// ported-from: test/libsolidity/semanticTests/inlineAssembly/mcopy_as_identifier_pre_cancun.sol
// ported-from: test/libsolidity/semanticTests/inlineAssembly/tload_tstore_not_reserved_before_cancun.sol

contract PreCancunBuiltinIdentifiers {
    function variables() public pure returns (uint result) {
        assembly {
            let mcopy := 1
            let blobhash := 2
            let blobbasefee := 4
            let tload := 8
            let tstore := 16
            result := add(add(mcopy, blobhash), add(blobbasefee, add(tload, tstore)))
        }
    }

    function functions() public pure returns (uint result) {
        assembly {
            function mcopy() -> r {
                r := 1
            }
            function blobhash() -> r {
                r := 2
            }
            function blobbasefee() -> r {
                r := 4
            }
            function tload() -> r {
                r := 8
            }
            function tstore() -> r {
                r := 16
            }
            result := add(
                add(mcopy(), blobhash()), add(blobbasefee(), add(tload(), tstore()))
            )
        }
    }
}
