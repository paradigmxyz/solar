//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: clearArray() => 1, 2, 3, 0, 0, 0
//@[none, gas, size] run-call: clearReference() => 1, 2, 3, 0, 0, 0
//@[none, gas, size] run-call: clearMapping() => 0, 0, 0
//@[none, gas, size] run-call: assign() => 4, 21862, 7
//@[none, gas, size] run-call: clearDirtyWord() => 0
//@[none, gas, size] run-call: clearStructWithMapping() => 0, 0, 17, 23

contract StorageDeletePackedStruct {
    struct Pair {
        uint8 first;
        uint16 middle;
        uint8 last;
    }

    Pair[] private pairs;
    mapping(uint256 => Pair) private mapped;
    Pair private direct;

    struct WithMapping {
        uint256 value;
        mapping(uint256 => uint256) entries;
        uint8 tail;
    }

    WithMapping private withMapping;

    function clearArray() external returns (uint8, uint16, uint8, uint8, uint16, uint8) {
        pairs.push();
        pairs[0] = Pair(1, 2, 3);
        pairs.push();
        pairs[1] = Pair(4, 5, 6);
        delete pairs[1];
        return (pairs[0].first, pairs[0].middle, pairs[0].last, pairs[1].first, pairs[1].middle, pairs[1].last);
    }

    function clearReference() external returns (uint8, uint16, uint8, uint8, uint16, uint8) {
        pairs.push();
        pairs[0] = Pair(1, 2, 3);
        pairs.push();
        pairs[1] = Pair(4, 5, 6);
        Pair storage pair = pairs[1];
        delete pair;
        return (pairs[0].first, pairs[0].middle, pairs[0].last, pairs[1].first, pairs[1].middle, pairs[1].last);
    }

    function clearMapping() external returns (uint8, uint16, uint8) {
        mapped[1] = Pair(4, 5, 6);
        delete mapped[1];
        return (mapped[1].first, mapped[1].middle, mapped[1].last);
    }

    function assign() external returns (uint8, uint16, uint8) {
        Pair memory pair = Pair(4, 0x5566, 7);
        pairs.push();
        pairs[0] = pair;
        return (pairs[0].first, pairs[0].middle, pairs[0].last);
    }

    function clearDirtyWord() external returns (uint256 word) {
        assembly {
            sstore(direct.slot, not(0))
        }
        delete direct;
        assembly {
            word := sload(direct.slot)
        }
    }

    function clearStructWithMapping()
        external
        returns (uint256 value, uint8 tail, uint256 baseWord, uint256 entry)
    {
        mapping(uint256 => uint256) storage entries = withMapping.entries;
        withMapping.value = 1;
        withMapping.tail = 2;
        entries[1] = 23;
        assembly {
            sstore(entries.slot, 17)
        }
        delete withMapping;
        value = withMapping.value;
        tail = withMapping.tail;
        entry = entries[1];
        assembly {
            baseWord := sload(entries.slot)
        }
    }
}
