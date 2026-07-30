//@run-call: word [1, 2, 3] => [2, 3]
//@run-call: bytesLocal 0x001122 => 0x1122
//@run-call: boolLocal [false, true, false] => 21
//@run-call: fixedLocal [[1, 2], [3, 4]] => 34

contract CalldataArraySubslice {
    function word(uint256[] calldata a) external pure returns (uint256[] memory) {
        return a[1:];
    }

    function bytesLocal(bytes calldata data) external pure returns (bytes memory) {
        bytes memory result = data[1:];
        return result;
    }

    function boolLocal(bool[] calldata values) external pure returns (uint256) {
        bool[] memory result = values[1:];
        return result.length * 10 + (result[0] ? 1 : 0) + (result[1] ? 2 : 0);
    }

    function fixedLocal(uint256[2][] calldata values) external pure returns (uint256) {
        uint256[2][] memory result = values[1:];
        return result[0][0] * 10 + result[0][1];
    }
}
