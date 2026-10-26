//@ codegen-matrix: standard
// `abi.decode` takes its input end from a calldata slice length that inline assembly can set up
// to `2**255 - 1`, so the head offset of a dynamic value is not bounded by the input and must stay
// below `2**64` as in solc. An offset of `2**64 - 1` is accepted; the calldata read it points at
// is past the end of the calldata and yields a zero length.
//@ run-call: bytesLen 0x0000000000000000000000000000000000000000000000000000000000000020 => 0
//@ run-call: bytesLen 0x000000000000000000000000000000000000000000000000ffffffffffffffff => 0
//@ run-call-fail: bytesLen 0x0000000000000000000000000000000000000000000000010000000000000000 => 0x
//@ run-call-fail: bytesLen 0x0000000000000000000000000000000100000000000000000000000000000000 => 0x
//@ run-call: arrayLen 0x0000000000000000000000000000000000000000000000000000000000000020 => 0
//@ run-call: arrayLen 0x000000000000000000000000000000000000000000000000ffffffffffffffff => 0
//@ run-call-fail: arrayLen 0x0000000000000000000000000000000000000000000000010000000000000000 => 0x
//@ run-call-fail: arrayLen 0x0000000000000000000000000000000100000000000000000000000000000000 => 0x

contract AbiDecodeLongInputOffset {
    function bytesLen(bytes calldata data) external pure returns (uint256) {
        bytes calldata input = data;
        assembly {
            input.length := sub(shl(255, 1), 1)
        }
        return abi.decode(input, (bytes)).length;
    }

    function arrayLen(bytes calldata data) external pure returns (uint256) {
        bytes calldata input = data;
        assembly {
            input.length := sub(shl(255, 1), 1)
        }
        return abi.decode(input, (uint256[])).length;
    }
}
