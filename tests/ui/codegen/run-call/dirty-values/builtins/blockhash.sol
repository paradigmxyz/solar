//@compile-flags: -O none -Zdump=mir
//@filecheck:

// `blockhash` and `blobhash` read their argument as a whole word, so a `uint64` that inline
// assembly left with bits above 64 must be cleaned before them, as solc does. The run-call EVM
// runs at block 0, where `blockhash` is always zero, so this checks the MIR.

contract C {
    // CHECK-LABEL: fn @viaBlockhash{{[( ]}}
    // CHECK: [[LOW:v[0-9]+]] = trunc i256 {{.*}} to i64
    // CHECK: zext i64 [[LOW]] to i256
    // CHECK: blockhash {{v[0-9]+}}
    function viaBlockhash(uint256 raw) external view returns (bytes32) {
        uint64 n;
        assembly { n := raw }
        return blockhash(n);
    }

    // CHECK-LABEL: fn @viaBlobhash{{[( ]}}
    // CHECK: [[LOW:v[0-9]+]] = trunc i256 {{.*}} to i64
    // CHECK: zext i64 [[LOW]] to i256
    // CHECK: blobhash {{v[0-9]+}}
    function viaBlobhash(uint256 raw) external view returns (bytes32) {
        uint64 n;
        assembly { n := raw }
        return blobhash(n);
    }
}
