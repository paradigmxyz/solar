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
            default { sstore(0, 255) }
        }
    }
}
