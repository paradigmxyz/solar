//@ compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@ filecheck: --check-prefixes=MIR,RUNTIME

contract CodeSizeData {
    // MIR-LABEL: fn @outer{{[( ]}}
    // MIR: datacopy literal_0,
    function outer() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    // MIR-LABEL: fn @inner{{[( ]}}
    // MIR: datacopy literal_1,
    function inner() external pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ";
    }

    // MIR-LABEL: fn @lastDataByte{{[( ]}}
    // MIR: codesize
    function lastDataByte() external pure returns (uint256 result) {
        assembly {
            codecopy(0, sub(codesize(), 1), 1)
            result := byte(0, mload(0))
        }
    }
}

// RUNTIME-LABEL: @module CodeSizeData_runtime
// `pack-data` packs the data of a module that reads `codesize`: `inner` copies from inside
// `literal_0`, so no test depends on a byte of the code.
// RUNTIME: literal_0: hex"
// RUNTIME-NOT: literal_1: hex"
