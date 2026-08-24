//@ revisions: shanghai cancun
//@[shanghai] compile-flags: --evm-version shanghai
//@[cancun] compile-flags: --evm-version cancun

contract C {
    function variables() external pure {
        assembly {
            let mcopy
            //~[cancun]^ ERROR: expected identifier, found Yul EVM builtin keyword `mcopy`
            let blobhash
            //~[cancun]^ ERROR: expected identifier, found Yul EVM builtin keyword `blobhash`
            let blobbasefee
            //~[cancun]^ ERROR: expected identifier, found Yul EVM builtin keyword `blobbasefee`
            let tload
            //~[cancun]^ ERROR: expected identifier, found Yul EVM builtin keyword `tload`
            let tstore
            //~[cancun]^ ERROR: expected identifier, found Yul EVM builtin keyword `tstore`
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
