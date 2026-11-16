//@ codegen-matrix: standard parisGas parisSize
//@[parisGas] compile-flags: --evm-version paris -O gas
//@[parisSize] compile-flags: --evm-version paris -O size
//@ run-call: calldataWords [] => 0xaabb
//@ run-call: calldataWords [1, 2, 3] => 0xaa000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003bb
//@ run-call: calldataSignedHash [-1, 2] => 0x38b5b2ceac7637132d27514ffcf440b705287635075af7b8bd5adcaa6a4cc5bb
//@ run-call: calldataNested [[1, 2], [3, 4]] => 0x0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000004
//@ run-call: calldataFixed [1, 2, 3] => 0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003
//@ run-call: calldataNarrow [1, 2] => 0x00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002
//@ run-call: memoryWords [1, 2, 3] => 0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003
//@ run-call: memoryFixed [1, 2, 3] => 0xaa000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003
//@ run-call: memoryNested [[1, 2], [3, 4]] => 0x0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000004
//@ run-call: wrappedWords => 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call: wrappedSigned => 0x0000000000000000000000000000000000000000000000000000000000000000b2f5e3d600000000000000000000000000000000000000000000000000000000
//@ run-call: wrappedNested => 0x00000000000000000000000000000000000000000000000000000000000000000c53737c00000000000000000000000000000000000000000000000000000000

contract AbiPackedWordArrayBlock {
    function calldataWords(uint256[] calldata values) external pure returns (bytes memory) {
        return abi.encodePacked(hex"aa", values, hex"bb");
    }

    function calldataSignedHash(int256[] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    function calldataNested(uint256[2][] calldata values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
    }

    function calldataFixed(uint256[3] calldata values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
    }

    function calldataNarrow(uint8[] calldata values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
    }

    function memoryWords(uint256[] memory values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
    }

    function memoryFixed(uint256[3] memory values) external pure returns (bytes memory) {
        return abi.encodePacked(hex"aa", values);
    }

    function memoryNested(uint256[2][2] memory values) external pure returns (bytes memory) {
        return abi.encodePacked(values);
    }

    function wrappedWords() external pure returns (bytes memory) {
        uint256[] calldata values;
        assembly {
            values.offset := sub(0, 32)
            values.length := 2
        }
        return abi.encodePacked(values);
    }

    function wrappedNested() external pure returns (bytes memory) {
        int256[2][] calldata values;
        assembly {
            values.offset := sub(0, 32)
            values.length := 1
        }
        return abi.encodePacked(values);
    }

    function wrappedSigned() external pure returns (bytes memory) {
        int256[] calldata values;
        assembly {
            values.offset := sub(0, 32)
            values.length := 2
        }
        return abi.encodePacked(values);
    }
}
