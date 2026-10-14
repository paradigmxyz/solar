contract DebugTracking {
    struct Leaf { uint16 small; bytes payload; }
    struct Tree { Leaf leaf; uint32[] nodes; }
    uint256[20] slots;

    function memoryView(Tree memory object) external pure returns (uint256) {
        return object.leaf.small + object.leaf.payload.length + object.nodes.length;
    }

    function calldataView(bytes calldata input, uint16[] calldata items) external pure returns (uint256) {
        return input.length + items.length;
    }

    function mutateObject(Tree memory object, uint16 value) external pure returns (uint256) {
        object.leaf.small = value;
        return uint256(keccak256(abi.encode(object)));
    }

    function twoCalls(uint256 x, uint256 y) external pure returns (uint256) {
        unchecked { return bump(x) + bump(y); }
    }

    function bump(uint256 input) internal pure returns (uint256) {
        unchecked { uint256 adjusted = (input * input) ^ 17; return adjusted + 1; }
    }

    function scoped(uint256 x, bool choose) external pure returns (uint256) {
        uint256 value = x;
        if (choose) {
            uint256 value = x + 2;
            x = value * 3;
        }
        return value + x;
    }

    function spill(uint256 index) external returns (uint256) {
        uint256 a0 = slots[0];
        uint256 a1 = slots[1];
        uint256 a2 = slots[2];
        uint256 a3 = slots[3];
        uint256 a4 = slots[4];
        uint256 a5 = slots[5];
        uint256 a6 = slots[6];
        uint256 a7 = slots[7];
        uint256 a8 = slots[8];
        uint256 a9 = slots[9];
        uint256 a10 = slots[10];
        uint256 a11 = slots[11];
        uint256 a12 = slots[12];
        uint256 a13 = slots[13];
        uint256 a14 = slots[14];
        uint256 a15 = slots[15];
        uint256 a16 = slots[16];
        uint256 a17 = slots[17];
        uint256 a18 = slots[18];
        uint256 a19 = slots[19];
        slots[index] = 0;
        unchecked {
            return a0 + a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9
                + a10 + a11 + a12 + a13 + a14 + a15 + a16 + a17 + a18 + a19;
        }
    }
}
