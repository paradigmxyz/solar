//@ compile-flags: -Zcodegen -O gas -Zdump=evm-ir -Zswitch-max-gas-code-growth=100
//@ filecheck:

contract SwitchSharedGrowthBudget {
    // CHECK-LABEL: @module deployment
    // CHECK-NOT: indexed_jump
    // CHECK-LABEL: @module runtime
    // CHECK: indexed_jump
    constructor(uint256 key) {
        assembly {
            switch key
            case 1 { sstore(0, 300) }
            case 7920 { sstore(0, 301) }
            case 15839 { sstore(0, 302) }
            case 23758 { sstore(0, 303) }
            case 31677 { sstore(0, 304) }
            case 39596 { sstore(0, 305) }
            case 47515 { sstore(0, 306) }
        }
    }

    function select(uint256 key) external pure returns (uint256 result) {
        assembly {
            switch key
            case 1 { result := 300 }
            case 7920 { result := 301 }
            case 15839 { result := 302 }
            case 23758 { result := 303 }
            case 31677 { result := 304 }
            case 39596 { result := 305 }
            case 47515 { result := 306 }
            default { result := 999 }
        }
    }
}
