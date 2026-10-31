//@ compile-flags: -Osize -Zdump=mir,evm-ir-runtime
//@ filecheck: --check-prefixes=MIR,RUNTIME
//@ run-call: lastDataByte => 90

contract CodeSizeData {
    // MIR-LABEL: fn @outer{{[( ]}}
    // MIR: datacopy literal_0,
    function outer() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAk9p34y6n3wd25rq4f5zr37e3p3e2z8iq9y7jzb6cn6z43dvyrkttnjfbf5jxvlsi47wqal9vq24zklmvt45hu43jsiom1tma7v3di8fppv5aspzh8rzhqmoe95b9ee0vbgi09qynk8w3tzpppp6up3c4dsa7l3600123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    // MIR-LABEL: fn @inner{{[( ]}}
    // MIR: datacopy literal_1,
    function inner() external pure returns (bytes memory) {
        return "k9p34y6n3wd25rq4f5zr37e3p3e2z8iq9y7jzb6cn6z43dvyrkttnjfbf5jxvlsi47wqal9vq24zklmvt45hu43jsiom1tma7v3di8fppv5aspzh8rzhqmoe95b9ee0vbgi09qynk8w3tzpppp6up3c4dsa7l360Z";
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
// RUNTIME: literal_0: hex"
// RUNTIME: literal_1: hex"
