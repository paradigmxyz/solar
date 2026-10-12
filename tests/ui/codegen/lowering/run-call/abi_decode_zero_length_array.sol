//@ codegen-matrix: standard debug
//@[debug] compile-flags: --revert-strings debug
//@[gas,size,mir] run-call-fail: decodeMemory 0x
//@[debug] run-call-fail: decodeMemory 0x => Error("ABI decoding: invalid calldata array offset")
//@ run-call: decodeMemory 0x0000000000000000000000000000000000000000000000000000000000000000 => 1
//@[gas,size,mir] run-call-fail: decodeCalldata 0x
//@[debug] run-call-fail: decodeCalldata 0x => Error("ABI decoding: invalid calldata array offset")
//@ run-call: decodeCalldata 0x0000000000000000000000000000000000000000000000000000000000000000 => 2
//@[gas,size,mir] run-call-fail: decodeBefore 0x
//@[debug] run-call-fail: decodeBefore 0x => Error("ABI decoding: tuple data too short")
//@ run-call: decodeBefore 0x0000000000000000000000000000000000000000000000000000000000000005 => 5
//@[gas,size,mir] run-call-fail: decodeAfter 0x0000000000000000000000000000000000000000000000000000000000000005
//@[debug] run-call-fail: decodeAfter 0x0000000000000000000000000000000000000000000000000000000000000005 => Error("ABI decoding: invalid calldata array offset")
//@ run-call: decodeAfter 0x00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000000 => 5
//@[gas,size,mir] run-call-fail: decodeDynamicElements 0x0000000000000000000000000000000000000000000000000000000000000020
//@[debug] run-call-fail: decodeDynamicElements 0x0000000000000000000000000000000000000000000000000000000000000020 => Error("ABI decoding: invalid calldata array offset")
//@ run-call: decodeDynamicElements 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 4
//@[gas,size,mir] run-call-fail: decodeNested 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020
//@[debug] run-call-fail: decodeNested 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020 => Error("ABI decoding: invalid calldata array offset")
//@ run-call: decodeNested 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000 => 1

// Like solc, decoding an empty array checks the word at its start. `decodeMemory` and
// `decodeCalldata` share one decoder.

contract AbiDecodeZeroLengthArray {
    function decodeMemory(bytes memory data) external pure returns (uint256) {
        abi.decode(data, (uint256[0]));
        return 1;
    }

    function decodeCalldata(bytes calldata data) external pure returns (uint256) {
        abi.decode(data, (uint256[0]));
        return 2;
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
