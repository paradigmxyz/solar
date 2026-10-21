//@ codegen-matrix: standard native
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@ run-call: keepsHeap 5, 6 => 1
//@ run-call: aliased => 1
//@ run-call: threeWords 5 => 5
//@ run-call: threeWordsKeepsHeap 5 => 1
//@ run-call: dynamicKeepsHeap 5 => 1
//@ run-call: literalReused => 50

// Static event data of at most two words is encoded in scratch memory. Wider
// static data and dynamic data are staged at the free memory pointer without
// reserving it. In both, the allocation behind it and the free memory pointer
// stay unchanged.
contract EventScratchData {
    struct Pair {
        uint256 first;
        uint256 second;
    }

    event Two(uint256 a, uint256 b);
    event Three(uint256 a, uint256 b, uint256 c);
    event Dynamic(uint256 a, bytes b, uint256 c);
    event Literal(string message, uint256 value);

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

    function threeWordsKeepsHeap(uint256 a) external returns (uint256 ok) {
        uint256[2] memory buffer = [uint256(0x11), 0x22];
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        emit Three(a, a + 1, a + 2);
        uint256 afterEmit;
        assembly {
            afterEmit := mload(0x40)
        }
        ok = afterEmit == before && buffer[0] == 0x11 && buffer[1] == 0x22 ? 1 : 0;
    }

    function dynamicKeepsHeap(uint256 a) external returns (uint256 ok) {
        bytes memory data = new bytes(40);
        uint256 before;
        assembly {
            mstore(add(data, 0x20), 0x33)
            before := mload(0x40)
        }
        emit Dynamic(a, data, a + 1);
        uint256 afterEmit;
        uint256 first;
        assembly {
            afterEmit := mload(0x40)
            first := mload(add(data, 0x20))
        }
        ok = afterEmit == before && first == 0x33 && data.length == 40 ? 1 : 0;
    }

    // A literal that an event staged is still intact when it is used afterwards.
    function literalReused() external returns (uint256) {
        string memory message = "a literal string that is longer than a single word";
        emit Literal(message, 1);
        return bytes(message).length;
    }
}
