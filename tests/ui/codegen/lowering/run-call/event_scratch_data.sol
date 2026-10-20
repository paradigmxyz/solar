//@ codegen-matrix: standard
//@ run-call: keepsHeap 5, 6 => 1
//@ run-call: aliased => 1
//@ run-call: threeWords 5 => 5
//@ run-call: dynamic 5 => 5
//@ run-call: indexedAnonymous 5 => 5

// Static event data of at most two words is encoded in scratch memory: the
// allocation behind it and the free memory pointer stay unchanged. Three words
// and dynamic data keep the encoded buffer.
contract EventScratchData {
    struct Pair {
        uint256 first;
        uint256 second;
    }

    event Two(uint256 a, uint256 b);
    event Three(uint256 a, uint256 b, uint256 c);
    event Dynamic(uint256 a, bytes b);
    event Indexed(uint256 indexed id, address account, bool flag) anonymous;

    function keepsHeap(uint256 a, uint256 b) external returns (uint256 ok) {
        uint256[2] memory buffer = [uint256(0x11), 0x22];
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        emit Two(a, b);
        uint256 afterEmit;
        assembly {
            afterEmit := mload(0x40)
        }
        ok = afterEmit == before && buffer[0] == 0x11 && buffer[1] == 0x22 ? 1 : 0;
    }

    // The fields are read before the data words overwrite the memory they alias,
    // and the log leaves its words in scratch memory.
    function aliased() external returns (uint256 second) {
        Pair memory pair;
        assembly {
            mstore(0, 1)
            mstore(0x20, 2)
            pair := 0
        }
        emit Two(pair.second, pair.first);
        second = pair.second;
    }

    function threeWords(uint256 a) external returns (uint256) {
        emit Three(a, a + 1, a + 2);
        return a;
    }

    function dynamic(uint256 a) external returns (uint256) {
        emit Dynamic(a, "x");
        return a;
    }

    function indexedAnonymous(uint256 id) external returns (uint256) {
        emit Indexed(id, address(1), true);
        return id;
    }
}
