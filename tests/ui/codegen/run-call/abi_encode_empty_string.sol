//@ run-call: encode() => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000
//@ run-call: encodePacked() => 0x
// ported-from: test/libsolidity/semanticTests/abicoder/abi_encode_empty_string_not_bytes0.sol

contract AbiEncodeEmptyString {
    function encode() external pure returns (bytes memory) {
        return abi.encode("");
    }

    function encodePacked() external pure returns (bytes memory) {
        return abi.encodePacked("");
    }
}
