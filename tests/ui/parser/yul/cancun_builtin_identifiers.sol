//@ revisions: shanghai cancun
//@[shanghai] compile-flags: --evm-version shanghai
//@[cancun] compile-flags: --evm-version cancun

contract C {
    function identifier() external pure {
        assembly {
            let mcopy
            //~[cancun]^ ERROR: cannot use builtin function name `mcopy` as identifier name
            //~[shanghai]^^ WARN: `mcopy` will be promoted to a Yul reserved identifier
            let blobhash
            //~[cancun]^ ERROR: cannot use builtin function name `blobhash` as identifier name
            //~[shanghai]^^ WARN: `blobhash` will be promoted to a Yul reserved identifier
            let blobbasefee
            //~[cancun]^ ERROR: cannot use builtin function name `blobbasefee` as identifier name
            //~[shanghai]^^ WARN: `blobbasefee` will be promoted to a Yul reserved identifier
            let tload
            //~[cancun]^ ERROR: cannot use builtin function name `tload` as identifier name
            //~[shanghai]^^ WARN: `tload` will be promoted to a Yul reserved identifier
            let tstore
            //~[cancun]^ ERROR: cannot use builtin function name `tstore` as identifier name
            //~[shanghai]^^ WARN: `tstore` will be promoted to a Yul reserved identifier
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
