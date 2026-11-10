//@ codegen-matrix: standard
//@ run-call: PackedStructDse::preservesRawNeighbors => true

contract PackedStructDse {
    struct Box {
        uint8 value;
        uint8 other;
    }

    Box internal box;

    function preservesRawNeighbors() public returns (bool) {
        assembly {
            sstore(box.slot, not(0))
        }
        box = Box({value: 1, other: 2});

        uint256 raw;
        assembly {
            raw := sload(box.slot)
        }
        return uint8(raw) == 1 && uint8(raw >> 8) == 2 && raw >> 16 == type(uint240).max;
    }
}
