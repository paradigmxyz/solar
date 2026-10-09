//@ codegen-matrix: standard
//@ run-call: pointer => 0xb3de648b0000000000000000000000000000000000000000000000000000000000000007
//@ run-call: ternary true => 0xb3de648b0000000000000000000000000000000000000000000000000000000000000007
//@ run-call: ternary false => 0xe420264a0000000000000000000000000000000000000000000000000000000000000007

// The selector hashes a storage signature's contents, not its slot.

contract AbiEncodeWithSignatureStorage {
    string first = "f(uint256)";
    string second = "g(uint256)";

    function pointer() external view returns (bytes memory) {
        string storage signature = first;
        return abi.encodeWithSignature(signature, 7);
    }

    function ternary(bool condition) external view returns (bytes memory) {
        return abi.encodeWithSignature(condition ? first : second, 7);
    }
}
