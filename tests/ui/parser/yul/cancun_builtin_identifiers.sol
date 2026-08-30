//@ revisions: shanghai cancun
//@[shanghai] compile-flags: --evm-version shanghai
//@[cancun] compile-flags: --evm-version cancun

contract C {
    function identifier() external pure {
        assembly {
            let mcopy
            //~[shanghai]^ WARN: `mcopy` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier
            //~[cancun]| ERROR: expected identifier, found Yul EVM builtin keyword `mcopy`
        }
    }

    function builtins() external view {
        assembly {
            mcopy(0, 0, 0) //~[shanghai] ERROR: Yul builtin `mcopy` requires Cancun-compatible EVM
            pop(blobhash(0)) //~[shanghai] ERROR: Yul builtin `blobhash` requires Cancun-compatible EVM
            pop(blobbasefee()) //~[shanghai] ERROR: Yul builtin `blobbasefee` requires Cancun-compatible EVM
            pop(tload(0)) //~[shanghai] ERROR: Yul builtin `tload` requires Cancun-compatible EVM
            tstore(0, 0) //~[shanghai] ERROR: Yul builtin `tstore` requires Cancun-compatible EVM
        }
    }
}
