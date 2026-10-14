contract DebugBase {
    uint8 small;
    bool flag;
    int16 signedValue;
    bytes4 tag;
}

contract DebugVariables is DebugBase {
    enum Choice { No, Yes }
    type Amount is uint64;
    struct Point { uint8 x; uint16 y; string label; }
    struct Line { Point from; Point to; }
    struct Node { uint256 value; mapping(uint256 => Node) children; }
    Point point;
    Line line;
    Node root;
    string text;
    uint8[] packed;
    uint16[17] fixedPacked;
    Point[] points;
    mapping(address => uint256) balances;
    mapping(string => Point) named;
    uint32 transient temporary;
    Choice choice;
    Amount amount;
    address payable receiver;
    function(uint256) external returns (uint256) callback;

    function compute(uint256 x, uint256 y) external pure returns (uint256) {
        uint256 local;
        unchecked { local = x + y; return local * x; }
    }

    function other(uint256 a, uint256 b) external pure returns (uint256) {
        uint256 intermediate;
        unchecked { intermediate = a + b; return intermediate * a; }
    }

    function mutate(uint256 x) external pure returns (uint256) {
        unchecked { return x + (x = 4); }
    }

    function afterWrite(uint256 x, uint256 y) external pure returns (uint256) {
        x = 4;
        unchecked { return x * y; }
    }

    function read(bytes memory input) external pure returns (uint256) {
        return input.length;
    }

    function maybeBytes(bool flag_) external pure returns (uint256) {
        bytes memory buffer;
        if (flag_) buffer = bytes("hi");
        assembly { mstore(0, 123) }
        return buffer.length;
    }

    function signed(int16 x, bytes4 prefix, bool flag_) external pure returns (int16, bytes4, bool) {
        unchecked { return (x + 1, prefix | bytes4(0x01000000), !flag_); }
    }
}

contract DebugImmutables {
    uint64 immutable value;
    bytes4 immutable prefix;
    uint256 constant SCALE = 10;
    constructor(uint64 v, bytes4 p) { value = v; prefix = p; }
    function read() external view returns (uint64, bytes4, uint256) {
        return (value, prefix, SCALE);
    }
}
