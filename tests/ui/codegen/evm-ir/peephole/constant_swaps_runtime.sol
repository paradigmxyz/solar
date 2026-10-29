//@ codegen-matrix: standard
//@ run-call: fingerprint 7, 9 => 0x45b5a19d68e9351b43be99482cfb876fa7eda185dc82d6130fd932a516be61f2
//@ run-call: fingerprint 0, 0 => 0x9446e6dd6d4fabf8abc913dc6e0bfa255d51d9321726a864860d76b82a04c99b
//@ run-call: fingerprint 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x69f317fe4b2b7de76f6c4ac60516a7289a55f45db25857db1561b004892ba392

contract ConstantSwaps {
    struct Item {
        uint256 tag;
        uint256 data;
    }

    struct Items {
        Item[] values;
    }

    struct Stack {
        Items values;
        uint256 remainder;
    }

    struct Pair {
        uint256 first;
        uint256 second;
    }

    struct State {
        uint256 status;
        Stack value;
        Pair multi;
        Stack internalValue;
        Pair frame;
        Pair frameMulti;
        uint256 globalValue;
        uint256 moduleIndex;
        uint256 functionIndex;
        uint256 pc;
        uint256 recovery;
        uint256 root;
    }

    // Constructing the nested object keeps pointers and incoming values live across the final
    // field stores. The scheduled stores exercise the constant-addition swap rewrite.
    function fingerprint(uint256 x, uint256 y) public pure returns (bytes32) {
        Item[] memory items = new Item[](3);
        items[0] = Item(4, 0);
        items[1] = Item(0, 0);
        items[2] = Item(0, 0);
        Items memory array = Items(items);
        Stack memory values = Stack(array, 0);
        Stack memory internalValue;
        Pair memory frame;
        Pair memory multi = Pair(type(uint256).max, 0);
        State memory state = State(
            0, values, multi, internalValue, frame, multi, x, 0, 0, 0,
            type(uint256).max, y
        );
        return keccak256(abi.encode(state));
    }
}
