//@ compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@ filecheck: --check-prefixes=MIR,RUNTIME
//@ run-call: lastDataByte() => 90

contract CodeSizeData {
    // MIR-LABEL: fn @outer{{[( ]}}
    // MIR: data_copy literal_0,
    function outer() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    // MIR-LABEL: fn @inner{{[( ]}}
    // MIR: data_copy literal_1,
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

// RUNTIME-LABEL: @module runtime
// RUNTIME: @data literal_0 hex"
// RUNTIME: @data literal_1 hex"
