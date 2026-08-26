//@ codegen-matrix: standard
//@ run-call: length() => 40
//@ run-call: firstWord() => 0x00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff

contract ConstantBytesValue {
    bytes internal constant DATA =
        hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff0102030405060708";

    function length() external pure returns (uint256) {
        bytes memory value = DATA;
        return value.length;
    }

    function firstWord() external pure returns (bytes32 result) {
        bytes memory value = DATA;
        assembly {
            result := mload(add(value, 32))
        }
    }
}
