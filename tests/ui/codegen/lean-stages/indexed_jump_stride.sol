//@ compile-flags: -Ogas --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// A switch whose targets lie past the code's first 256 bytes jumps through a stride table: entry
// stubs `JUMPDEST; PUSH2 target; JUMP` after the last block, into which the index is scaled.
// It exports as `jumpTable` like a packed table; the stubs are the assembler's.

contract LeanStagesIndexedJumpStride {
    function select(uint256 key) external {
        assembly {
            switch key
            case 0 { sstore(0, 4096) sstore(add(0, 100), 8192) sstore(add(0, 200), 16384) log1(0, 0, 12288) }
            case 1 { sstore(1, 4097) sstore(add(1, 100), 8193) sstore(add(1, 200), 16385) log1(0, 0, 12289) }
            case 2 { sstore(2, 4098) sstore(add(2, 100), 8194) sstore(add(2, 200), 16386) log1(0, 0, 12290) }
            case 3 { sstore(3, 4099) sstore(add(3, 100), 8195) sstore(add(3, 200), 16387) log1(0, 0, 12291) }
            case 4 { sstore(4, 4100) sstore(add(4, 100), 8196) sstore(add(4, 200), 16388) log1(0, 0, 12292) }
            case 5 { sstore(5, 4101) sstore(add(5, 100), 8197) sstore(add(5, 200), 16389) log1(0, 0, 12293) }
            case 6 { sstore(6, 4102) sstore(add(6, 100), 8198) sstore(add(6, 200), 16390) log1(0, 0, 12294) }
            case 7 { sstore(7, 4103) sstore(add(7, 100), 8199) sstore(add(7, 200), 16391) log1(0, 0, 12295) }
            default { sstore(0, 255) }
        }
    }
}
