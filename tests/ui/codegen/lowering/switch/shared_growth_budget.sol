//@ compile-flags: -Zcodegen -O gas -Zdump=evm-ir -Zswitch-max-gas-code-growth=100 -Zswitch-max-bit-slice-gas-code-growth=63
//@ filecheck:

contract SwitchSharedGrowthBudget {
    // CHECK-LABEL: @module deployment
    // CHECK-NOT: indexed_jump
    // CHECK-LABEL: @module runtime
    // CHECK: indexed_jump
    constructor(uint256 key) {
        assembly {
            switch key
            case 0xcbf99d38 { sstore(0, 300) }
            case 0x87d912cb { sstore(0, 301) }
            case 0x920f5c73 { sstore(0, 302) }
            case 0x41052a0d { sstore(0, 303) }
            case 0x7238232f { sstore(0, 304) }
            case 0x905f7d67 { sstore(0, 305) }
            case 0x3b88f6c2 { sstore(0, 306) }
        }
    }

    function select(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 0xcbf99d38 { result := 300 }
            case 0x87d912cb { result := 301 }
            case 0x920f5c73 { result := 302 }
            case 0x41052a0d { result := 303 }
            case 0x7238232f { result := 304 }
            case 0x905f7d67 { result := 305 }
            case 0x3b88f6c2 { result := 306 }
            default { result := 999 }
        }
    }
}
