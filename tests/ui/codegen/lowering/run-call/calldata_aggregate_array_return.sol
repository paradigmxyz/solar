//@ codegen-matrix: standard
//@ run-call: structs [(1, 2), (3, 4)] => [(1, 2), (3, 4)]
//@ run-call: structs [] => []
//@ run-call: slice [(1, 2), (3, 4), (5, 6)] => [(3, 4)]
//@ run-call: rows [[1, 2], [3, 4]] => [[1, 2], [3, 4]]
//@ run-call: nested [[1], [], [2, 3]] => [[1], [], [2, 3]]
//@ run-call: narrow [(7, true), (255, false)] => [(7, true), (255, false)]
//@ run-call: pair [(1, 2)] => 9, [(1, 2)]
//@ run-call-fail: 0xe999a4e40000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000001

// A returned calldata array of structs or arrays is copied to memory, with
// each element validated, before it is encoded. The encoder reads only
// calldata arrays of words or bytes in place.
contract CalldataAggregateArrayReturn {
    struct Pair {
        uint256 a;
        uint256 b;
    }

    struct Narrow {
        uint8 a;
        bool b;
    }

    function structs(Pair[] calldata p) external pure returns (Pair[] calldata) {
        return p;
    }

    function slice(Pair[] calldata p) external pure returns (Pair[] calldata) {
        return p[1:2];
    }

    function rows(uint256[2][] calldata p) public pure returns (uint256[2][] calldata) {
        return p;
    }

    function nested(uint256[][] calldata p) public pure returns (uint256[][] calldata r) {
        r = p;
    }

    function narrow(Narrow[] calldata p) external pure returns (Narrow[] calldata) {
        return p;
    }

    function pair(Pair[] calldata p) external pure returns (uint256, Pair[] calldata) {
        return (9, p);
    }
}
