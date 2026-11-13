// A dense switch, compiled to a jump table: `switch` on a key masks it to its bucket and jumps
// through a packed table of eight blocks (`indexed_jump`), each checking the key.
contract Switch {
    function select(uint256 key) external {
        assembly {
            switch key
            case 0 { sstore(0, 7) }
            case 1 { sstore(0, 10) }
            case 2 { sstore(0, 13) }
            case 3 { sstore(0, 16) }
            case 4 { sstore(0, 19) }
            case 5 { sstore(0, 22) }
            case 6 { sstore(0, 25) }
            case 7 { sstore(0, 28) }
            default { sstore(0, 1) }
        }
    }
}
