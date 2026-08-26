//@ run-call: PackedStructDse::preservesRawNeighbors() => true

contract PackedStructDse {
    struct Box {
        uint8 value;
    }

    Box internal box;

    function preservesRawNeighbors() public returns (bool) {
        assembly {
            sstore(box.slot, not(0))
        }
        box = Box({value: 1});

        uint256 raw;
        assembly {
            raw := sload(box.slot)
        }
        return uint8(raw) == 1 && raw >> 8 == type(uint248).max;
    }
}
