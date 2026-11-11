//@ codegen-matrix: standard
//@ run-call: shrink() => true
//@ run-call: grow() => true
//@ run-call: boundary() => true

// A memory array copied over a packed storage array writes each slot word whole: the last slot
// holds only the new elements, and slots past the new length are cleared.
contract C {
    uint8[] bytesArray;
    uint80[] wordsArray;

    function shrink() external returns (bool) {
        uint8[] memory big = new uint8[](40);
        for (uint256 i = 0; i < 40; i++) {
            big[i] = uint8(i + 1);
        }
        bytesArray = big;

        uint8[] memory small = new uint8[](3);
        small[0] = 7;
        small[1] = 8;
        small[2] = 9;
        bytesArray = small;

        (uint256 first, uint256 second) = bytesWords();
        return first == 0x090807 && second == 0 && bytesArray.length == 3;
    }

    function grow() external returns (bool) {
        uint8[] memory small = new uint8[](3);
        small[0] = 7;
        small[1] = 8;
        small[2] = 9;
        bytesArray = small;

        uint8[] memory big = new uint8[](33);
        big[0] = 1;
        big[32] = 2;
        bytesArray = big;

        (uint256 first, uint256 second) = bytesWords();
        return first == 1 && second == 2 && bytesArray[32] == 2;
    }

    function boundary() external returns (bool) {
        uint80[] memory values = new uint80[](4);
        values[0] = 1;
        values[2] = 3;
        values[3] = type(uint80).max;
        wordsArray = values;

        (uint256 first, uint256 second) = wordsWords();
        return first == (3 << 160) + 1 && second == type(uint80).max;
    }

    function bytesWords() private view returns (uint256 first, uint256 second) {
        assembly {
            mstore(0, bytesArray.slot)
            let data := keccak256(0, 32)
            first := sload(data)
            second := sload(add(data, 1))
        }
    }

    function wordsWords() private view returns (uint256 first, uint256 second) {
        assembly {
            mstore(0, wordsArray.slot)
            let data := keccak256(0, 32)
            first := sload(data)
            second := sload(add(data, 1))
        }
    }
}
