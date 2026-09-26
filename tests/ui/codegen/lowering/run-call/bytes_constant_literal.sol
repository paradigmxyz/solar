//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: packedHash 0x1111111111111111111111111111111111111111 => 0x4f83b61c54ed45c71d48b141aacb77f249501a6ee69d07f5ea1541348e30be2f
//@ run-call: packed 0x1111111111111111111111111111111111111111 => 0x602c3d8160093d39f33d3d3d3d363d3d37363d7311111111111111111111111111111111111111115af43d3d93803e602a57fd5bf3
//@ run-call: word 7 => 0x616263000000000000000000000000000000000000000000000000000000000007
//@ run-call: name => 5, 0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8, "hello hello"

// A `bytes` or `string` constant packs, concatenates, hashes and measures as
// the literal it names: its bytes are immediates, not a buffer copied in. A
// `bytes32` constant initialized from a string literal is still a whole word.
contract BytesConstantLiteral {
    bytes private constant HEAD = hex"602c3d8160093d39f33d3d3d3d363d3d37363d73";
    bytes private constant TAIL = hex"5af43d3d93803e602a57fd5bf3";
    bytes32 private constant WORD = "abc";
    string private constant NAME = "hello";

    // CHECK-LABEL: fn @packedHash(
    // CHECK: keccak256_packed (data hex"602c3d8160093d39f33d3d3d3d363d3d37363d73", u160 {{v[0-9]+}}, data hex"5af43d3d93803e602a57fd5bf3")
    function packedHash(address a) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(HEAD, a, TAIL));
    }

    function packed(address a) external pure returns (bytes memory) {
        return abi.encodePacked(HEAD, a, TAIL);
    }

    function word(uint8 x) external pure returns (bytes memory) {
        return abi.encodePacked(WORD, x);
    }

    function name() external pure returns (uint256, bytes32, string memory) {
        return (bytes(NAME).length, keccak256(bytes(NAME)), string.concat(NAME, " ", NAME));
    }
}
