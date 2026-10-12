//@ codegen-matrix: standard
//@ run-call-fail: decodeMemory 0x
//@ run-call: decodeMemory 0x0000000000000000000000000000000000000000000000000000000000000000 => 1
//@ run-call-fail: decodeCalldata 0x
//@ run-call: decodeCalldata 0x0000000000000000000000000000000000000000000000000000000000000000 => 2
//@ run-call-fail: decodeCalldataAgain 0x
//@ run-call: decodeCalldataAgain 0x0000000000000000000000000000000000000000000000000000000000000000 => 3
//@ run-call-fail: decodeBefore 0x
//@ run-call: decodeBefore 0x0000000000000000000000000000000000000000000000000000000000000005 => 5
//@ run-call-fail: decodeAfter 0x0000000000000000000000000000000000000000000000000000000000000005
//@ run-call: decodeAfter 0x00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000000 => 5
//@ run-call-fail: decodeDynamicElements 0x0000000000000000000000000000000000000000000000000000000000000020
//@ run-call: decodeDynamicElements 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 4
//@ run-call-fail: decodeNested 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020
//@ run-call: decodeNested 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 1

// Like solc, decoding an empty array reads its first word.

contract AbiDecodeZeroLengthArray {
    function decodeMemory(bytes memory data) external pure returns (uint256) {
        abi.decode(data, (uint256[0]));
        return 1;
    }

    function decodeCalldata(bytes calldata data) external pure returns (uint256) {
        abi.decode(data, (uint256[0]));
        return 2;
    }

    function decodeCalldataAgain(bytes calldata data) external pure returns (uint256) {
        abi.decode(data, (uint256[0]));
        return 3;
    }

    function decodeBefore(bytes memory data) external pure returns (uint256) {
        (, uint256 x) = abi.decode(data, (uint256[0], uint256));
        return x;
    }

    function decodeAfter(bytes memory data) external pure returns (uint256) {
        (uint256 x,) = abi.decode(data, (uint256, uint256[0]));
        return x;
    }

    function decodeDynamicElements(bytes memory data) external pure returns (uint256) {
        abi.decode(data, (uint256[][0]));
        return 4;
    }

    function decodeNested(bytes memory data) external pure returns (uint256) {
        return abi.decode(data, (uint256[][0][])).length;
    }
}
