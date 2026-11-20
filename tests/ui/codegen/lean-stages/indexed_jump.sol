//@ compile-flags: -Ogas --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A dense switch jumps through a table of blocks by the index on the stack (`indexed_jump`),
// which exports as `jumpTable`.

contract LeanStagesIndexedJump {
    function select(uint256 key) external {
        assembly {
            switch key
            case 10 { sstore(0, 30) }
            case 11 { sstore(0, 33) }
            case 12 { sstore(0, 36) }
            case 13 { sstore(0, 39) }
            case 14 { sstore(0, 42) }
            case 15 { sstore(0, 45) }
            case 16 { sstore(0, 48) }
            case 17 { sstore(0, 51) }
            case 18 { sstore(0, 54) }
            case 19 { sstore(0, 57) }
            case 20 { sstore(0, 60) }
            case 21 { sstore(0, 63) }
            case 22 { sstore(0, 66) }
            case 23 { sstore(0, 69) }
            case 24 { sstore(0, 72) }
            case 25 { sstore(0, 75) }
            case 26 { sstore(0, 78) }
            case 27 { sstore(0, 81) }
            case 28 { sstore(0, 84) }
            case 29 { sstore(0, 87) }
            default { sstore(0, 255) }
        }
    }
}
