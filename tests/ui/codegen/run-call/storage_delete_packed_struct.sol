//@ run-call: clearArray() => 1, 2, 3, 0, 0, 0
//@ run-call: clearReference() => 1, 2, 3, 0, 0, 0
//@ run-call: clearMapping() => 0, 0, 0
//@ run-call: assign() => 4, 21862, 7

contract StorageDeletePackedStruct {
    struct Pair {
        uint8 first;
        uint16 middle;
        uint8 last;
    }

    Pair[] private pairs;
    mapping(uint256 => Pair) private mapped;

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
}
