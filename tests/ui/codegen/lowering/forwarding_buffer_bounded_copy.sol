//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck: --implicit-check-not=msize

// solmate copies `returndatasize()` bytes to `0` only under `case 32`, so the copy stays in
// scratch space. Neither the helper nor its caller needs a dynamic spill base.
// https://github.com/paradigmxyz/solar/issues/1625

function lastCallReturnedTrue() pure returns (bool success) {
    assembly {
        let returnDataSize := returndatasize()
        switch returnDataSize
        case 32 {
            returndatacopy(0, 0, returnDataSize)
            success := iszero(iszero(mload(0)))
        }
        case 0 { success := 1 }
        default { success := 0 }
    }
}

contract C {
    // CHECK-LABEL: C (runtime)
    // CHECK: returndatacopy
    fallback() external {
        uint256 v0;
        uint256 v1;
        uint256 v2;
        uint256 v3;
        uint256 v4;
        uint256 v5;
        uint256 v6;
        uint256 v7;
        uint256 v8;
        uint256 v9;
        uint256 v10;
        uint256 v11;
        uint256 v12;
        uint256 v13;
        uint256 v14;
        uint256 v15;
        uint256 v16;
        uint256 v17;
        assembly {
            v0 := sload(0)
            v1 := sload(1)
            v2 := sload(2)
            v3 := sload(3)
            v4 := sload(4)
            v5 := sload(5)
            v6 := sload(6)
            v7 := sload(7)
            v8 := sload(8)
            v9 := sload(9)
            v10 := sload(10)
            v11 := sload(11)
            v12 := sload(12)
            v13 := sload(13)
            v14 := sload(14)
            v15 := sload(15)
            v16 := sload(16)
            v17 := sload(17)
        }
        bool ok = lastCallReturnedTrue();
        uint256 r = v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7
            ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14 ^ v15 ^ v16 ^ v17;
        assembly {
            mstore(0, add(r, ok))
            return(0, 32)
        }
    }
}
