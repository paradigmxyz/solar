//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: threeWords 5 => 1
//@ run-call: dynamic 5 => 1
//@ run-call: literal =>

// Static event data of three or more words and dynamic event data are staged
// at the free memory pointer without reserving it: the pointer and the
// allocations below it stay unchanged, and no length word is stored.
contract EventStagedData {
    event Three(uint256 a, uint256 b, uint256 c);
    event Dynamic(uint256 a, bytes b, uint256 c);
    event Literal(string message, uint256 value);

    // CHECK-LABEL: fn @threeWords{{[( ]}}
    // CHECK: abi_encode [word, word, word], scratch, args
    // CHECK-NOT: alloc
    // CHECK: log
    function threeWords(uint256 a) external returns (uint256 ok) {
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

    // CHECK-LABEL: fn @dynamic{{[( ]}}
    // CHECK: abi_encode [word, memory_bytes, word], scratch, args
    // CHECK-NOT: alloc
    // CHECK: log
    function dynamic(uint256 a) external returns (uint256 ok) {
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

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: abi_encode [memory_bytes, word], scratch, args
    // CHECK: log
    function literal() external {
        emit Literal("a literal string that is longer than a single word", 1);
    }
}
