//@ codegen-matrix: standard
//@ run-call: array => 9
//@ run-call: structure => 9
//@ run-call: nestedElement => 9
//@ run-call: field => 9
//@ run-call: mappingValue => 9
//@ run-call: ternary => 9
//@ run-call: text => 98
//@ run-call: functionPointer => 9
//@ run-call: libraryCall => 9
//@ run-call: attachedReceiver => 1
// A storage array, struct, `bytes`, or `string` argument is copied to memory after the arguments
// that follow it ran, as solc does via IR. The receiver of an attached call is copied first.
struct S {
    uint256 a;
}

struct Holder {
    uint256[] items;
}

library L {
    function sum(uint256[] memory m, uint256 z) internal pure returns (uint256) {
        return m[0] + z;
    }
}

contract Test {
    using L for uint256[];

    uint256[] sa;
    uint256[] sb;
    uint256[][] sa2;
    S s;
    Holder h;
    bytes sbytes;
    mapping(uint256 => uint256[]) mp;

    constructor() {
        sa.push(1);
        sb.push(1);
        sa2.push();
        sa2[0].push(1);
        s.a = 1;
        h.items.push(1);
        sbytes = "a";
        mp[1].push(1);
    }

    function w() internal returns (uint256) {
        sa[0] = 9;
        sa2[0][0] = 9;
        s.a = 9;
        h.items[0] = 9;
        sbytes = "b";
        mp[1][0] = 9;
        return 0;
    }

    function first(uint256[] memory m, uint256 z) internal pure returns (uint256) {
        return m[0] + z;
    }

    function firstOfStruct(S memory m, uint256 z) internal pure returns (uint256) {
        return m.a + z;
    }

    function firstByte(string memory m, uint256 z) internal pure returns (uint256) {
        return uint8(bytes(m)[0]) + z;
    }

    function array() external returns (uint256) {
        return first(sa, w());
    }

    function structure() external returns (uint256) {
        return firstOfStruct(s, w());
    }

    function nestedElement() external returns (uint256) {
        return first(sa2[0], w());
    }

    function field() external returns (uint256) {
        return first(h.items, w());
    }

    function mappingValue() external returns (uint256) {
        return first(mp[1], w());
    }

    function ternary() external returns (uint256) {
        bool c = true;
        return first(c ? sa : sb, w());
    }

    function text() external returns (uint256) {
        return firstByte(string(sbytes), w());
    }

    function functionPointer() external returns (uint256) {
        function(uint256[] memory, uint256) internal pure returns (uint256) fp = first;
        return fp(sa, w());
    }

    function libraryCall() external returns (uint256) {
        return L.sum(sa, w());
    }

    function attachedReceiver() external returns (uint256) {
        return sa.sum(w());
    }
}
