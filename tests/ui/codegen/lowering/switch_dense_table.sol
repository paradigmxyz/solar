//@ compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime
//@ filecheck: --check-prefix=TABLE

// TABLE: push 10
// TABLE-NEXT: swap1
// TABLE-NEXT: sub
// TABLE: indexed_jump
contract SwitchDenseTable {
    uint256 value;

    function select(uint256 key) external {
        assembly {
            switch key
            case 10 { sstore(0, 10) }
            case 11 { sstore(0, 11) }
            case 12 { sstore(0, 12) }
            case 13 { sstore(0, 13) }
            case 14 { sstore(0, 14) }
            case 15 { sstore(0, 15) }
            case 16 { sstore(0, 16) }
            case 17 { sstore(0, 17) }
            case 18 { sstore(0, 18) }
            case 19 { sstore(0, 19) }
            case 20 { sstore(0, 20) }
            case 21 { sstore(0, 21) }
            case 22 { sstore(0, 22) }
            case 23 { sstore(0, 23) }
            case 24 { sstore(0, 24) }
            case 25 { sstore(0, 25) }
            case 26 { sstore(0, 26) }
            case 27 { sstore(0, 27) }
            case 28 { sstore(0, 28) }
            case 29 { sstore(0, 29) }
            case 30 { sstore(0, 30) }
            case 31 { sstore(0, 31) }
            case 32 { sstore(0, 32) }
            case 33 { sstore(0, 33) }
            default { sstore(0, 255) }
        }
    }
}
