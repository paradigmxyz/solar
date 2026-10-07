//@ revisions: capped allowed
//@[capped] compile-flags: -O gas --evm-version paris -Zswitch-max-gas-code-growth=80 -Zdump=evm-ir
//@[capped] filecheck: --check-prefix=CAPPED
//@[allowed] compile-flags: -O gas --evm-version paris -Zswitch-max-gas-code-growth=81 -Zdump=evm-ir
//@[allowed] filecheck: --check-prefix=ALLOWED

// Before Shanghai, constructor tables use three-byte labels, so the model
// charges the outlined dense table over 0..19 with 81 bytes of growth over
// a linear scan. An 80-byte cap rejects it and an 81-byte cap accepts it.
contract ConstructorTable {
    // CAPPED-LABEL: @module ConstructorTable_deployment
    // CAPPED-NOT: indexed_jump

    // ALLOWED-LABEL: @module ConstructorTable_deployment
    // ALLOWED: push 20{{$}}
    // ALLOWED-NEXT: gt
    // ALLOWED: indexed_jump
    constructor(uint256 key) {
        assembly {
            switch key
            case 0 { sstore(0, 100) }
            case 3 { sstore(0, 101) }
            case 5 { sstore(0, 102) }
            case 7 { sstore(0, 103) }
            case 8 { sstore(0, 104) }
            case 18 { sstore(0, 105) }
            case 19 { sstore(0, 106) }
            default { sstore(0, 999) }
        }
    }
}
