//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: decode(bytes) 0x00000000000000000000000000000000000000000000000000000000000000210000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000761626364656667000000000000000000000000000000000000000000000000000000 => 33, 7, 0x61
// ported-from: test/libsolidity/semanticTests/abicoder/abi_decode_simple_storage.sol

contract AbiDecodeStorageSimple {
    bytes private data;

    function decode(bytes memory input) external returns (uint256 value, uint256 length, bytes1 first) {
        data = input;
        bytes memory decoded;
        (value, decoded) = abi.decode(data, (uint256, bytes));
        return (value, decoded.length, decoded[0]);
    }
}
