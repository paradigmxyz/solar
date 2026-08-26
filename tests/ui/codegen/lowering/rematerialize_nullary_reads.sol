//@ compile-flags: -O none -Zdump=evm-ir-runtime
//@ filecheck:

contract RematerializeNullaryReads {
    // Each value is defined once and used twice. Stable two-gas reads should be
    // re-emitted at both stores instead of being retained or spilled. `NUMBER`
    // preserves its evaluated value because instrumented EVMs can change it
    // across calls.
    // CHECK-COUNT-2: {{^ +}}calldatasize{{$}}
    // CHECK-COUNT-2: {{^ +}}codesize{{$}}
    // CHECK-COUNT-2: {{^ +}}caller{{$}}
    // CHECK-COUNT-2: {{^ +}}callvalue{{$}}
    // CHECK-COUNT-2: {{^ +}}address{{$}}
    // CHECK-COUNT-2: {{^ +}}origin{{$}}
    // CHECK-COUNT-2: {{^ +}}gasprice{{$}}
    // CHECK-COUNT-2: {{^ +}}coinbase{{$}}
    // CHECK-COUNT-2: {{^ +}}timestamp{{$}}
    // CHECK-COUNT-1: {{^ +}}number{{$}}
    // CHECK-COUNT-2: {{^ +}}prevrandao{{$}}
    // CHECK-COUNT-2: {{^ +}}gaslimit{{$}}
    // CHECK-COUNT-2: {{^ +}}chainid{{$}}
    // CHECK-COUNT-2: {{^ +}}basefee{{$}}
    // CHECK-COUNT-2: {{^ +}}blobbasefee{{$}}
    function readTwice() external payable {
        assembly {
            let v0 := calldatasize()
            mstore(0x00, v0)
            mstore(0x20, v0)
            let v1 := codesize()
            mstore(0x40, v1)
            mstore(0x60, v1)
            let v2 := caller()
            mstore(0x80, v2)
            mstore(0xa0, v2)
            let v3 := callvalue()
            mstore(0xc0, v3)
            mstore(0xe0, v3)
            let v4 := address()
            mstore(0x100, v4)
            mstore(0x120, v4)
            let v5 := origin()
            mstore(0x140, v5)
            mstore(0x160, v5)
            let v6 := gasprice()
            mstore(0x180, v6)
            mstore(0x1a0, v6)
            let v7 := coinbase()
            mstore(0x1c0, v7)
            mstore(0x1e0, v7)
            let v8 := timestamp()
            mstore(0x200, v8)
            mstore(0x220, v8)
            let v9 := number()
            mstore(0x240, v9)
            mstore(0x260, v9)
            let v10 := prevrandao()
            mstore(0x280, v10)
            mstore(0x2a0, v10)
            let v11 := gaslimit()
            mstore(0x2c0, v11)
            mstore(0x2e0, v11)
            let v12 := chainid()
            mstore(0x300, v12)
            mstore(0x320, v12)
            let v13 := basefee()
            mstore(0x340, v13)
            mstore(0x360, v13)
            let v14 := blobbasefee()
            mstore(0x380, v14)
            mstore(0x3a0, v14)
            return(0, 0x3c0)
        }
    }
}
