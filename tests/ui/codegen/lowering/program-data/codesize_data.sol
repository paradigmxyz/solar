//@ compile-flags: -Osize -Zdump=mir
//@ filecheck: --check-prefixes=MIR
//@ run-call: outerHash => 0xc81cb48a8688de2ee9a8ac2b809088114fe2b4292dd609b7a1003baccec36902
//@ run-call: innerHash => 0xfe104a769973081412d46a6d04c990a5e9cc804baf45fa43d99b7dbee24984b8
//@ run-call: hasCode => true

// `pack-data` packs the data of a module that reads `codesize`, so the layout of its code is free
// to change and no test depends on a byte of it.

contract CodeSizeData {
    // MIR-LABEL: fn @outer{{[( ]}}
    // MIR-NOT: datacopy
    function outer() public pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ!";
    }

    // MIR-LABEL: fn @inner{{[( ]}}
    // MIR-NOT: datacopy
    function inner() public pure returns (bytes memory) {
        return "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdefZ";
    }

    function outerHash() external pure returns (bytes32) {
        return keccak256(outer());
    }

    function innerHash() external pure returns (bytes32) {
        return keccak256(inner());
    }

    // MIR-LABEL: fn @hasCode{{[( ]}}
    // MIR: codesize
    function hasCode() external pure returns (bool result) {
        assembly {
            result := gt(codesize(), 0)
        }
    }
}
