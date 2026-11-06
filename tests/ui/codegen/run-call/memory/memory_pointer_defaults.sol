//@ codegen-matrix: standard
//@ run-call: defaultReturn => ((0, 0x, []), [], [0x, 0x], "")
//@ run-call: defaultLocals => ((0, 0x, []), [], [0x, 0x], ""), [(0, 0x, []), (0, 0x, [])], [([[0, 0], [0, 0]])]
//@ run-call: newArrays => [[], [7]], [0x, 0x], [(3, 0x, []), (0, 0x78, [])], 1
//@ run-call: decodeDefaults => ((0, 0x, []), [], [0x, 0x], "")
//@ run-call: deleteElement => [(0, 0x, [])]
//@ run-call: copyCalldata [(1, 0x01, [2]), (0, 0x, [])] => [(1, 0x01, [2]), (0, 0x, [])]
//@ run-call: copyFromStorage => ((0, 0x, []), [(0, 0x, [])], [0x, 0x6162], ""), [(0, 0x, []), (0, 0x, [5])], [[], [9]], [0x, 0x63]
//@ run-call: copyToStorage => [(0, 0x, []), (0, 0x, [])], [[], [4]], [0x, 0x]

// Memory pointers are never null: defaults, `new`, `delete`, ABI decoding, and
// copies from calldata or storage store a real object in every reference field
// and element. Each call dirties scratch memory first, so a zero pointer would
// read a nonzero length instead of an empty object.
contract MemoryPointerDefaults {
    struct Inner {
        uint256 value;
        bytes data;
        uint256[] values;
    }

    struct Outer {
        Inner inner;
        Inner[] list;
        bytes[2] pair;
        string text;
    }

    struct Grid {
        uint256[2][2] cells;
    }

    Outer stored;
    Inner[] storedList;
    uint256[][] storedNested;
    bytes[] storedBytes;

    function dirtyScratch() internal pure {
        assembly ("memory-safe") {
            mstore(0, 0x20)
            mstore(0x20, 0x20)
        }
    }

    function defaultReturn() external pure returns (Outer memory outer) {
        dirtyScratch();
    }

    function defaultLocals() external pure returns (Outer memory, Inner[2] memory, Grid[] memory) {
        dirtyScratch();
        Outer memory outer;
        Inner[2] memory pair;
        Grid[] memory grids = new Grid[](1);
        return (outer, pair, grids);
    }

    function newArrays()
        external
        pure
        returns (uint256[][] memory nested, bytes[] memory blobs, Inner[] memory inners, uint256)
    {
        dirtyScratch();
        nested = new uint256[][](2);
        blobs = new bytes[](2);
        inners = new Inner[](2);
        nested[1] = new uint256[](1);
        nested[1][0] = 7;
        Inner memory first = inners[0];
        first.value = 3;
        inners[1].data = "x";
        return (nested, blobs, inners, nested[0].length + blobs[1].length + inners[0].values.length + 1);
    }

    function decodeDefaults() external pure returns (Outer memory) {
        Outer memory outer;
        bytes memory encoded = abi.encode(outer);
        dirtyScratch();
        return abi.decode(encoded, (Outer));
    }

    function deleteElement() external pure returns (Inner[] memory inners) {
        dirtyScratch();
        inners = new Inner[](1);
        inners[0].data = "z";
        inners[0].values = new uint256[](1);
        delete inners[0];
    }

    function copyCalldata(Inner[] calldata source) external pure returns (Inner[] memory copy) {
        dirtyScratch();
        copy = source;
    }

    function copyFromStorage()
        external
        returns (Outer memory, Inner[] memory, uint256[][] memory, bytes[] memory)
    {
        stored.list.push();
        stored.pair[1] = "ab";
        storedList.push();
        storedList.push().values.push(5);
        storedNested.push();
        storedNested.push().push(9);
        storedBytes.push();
        storedBytes.push("c");
        dirtyScratch();
        return (stored, storedList, storedNested, storedBytes);
    }

    function copyToStorage() external returns (Inner[] memory, uint256[][] memory, bytes[] memory) {
        dirtyScratch();
        storedList = new Inner[](2);
        storedNested = new uint256[][](2);
        storedNested[1].push(4);
        storedBytes = new bytes[](2);
        dirtyScratch();
        return (storedList, storedNested, storedBytes);
    }
}
