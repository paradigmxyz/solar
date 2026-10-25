//@ codegen-matrix: standard
//@ run-call: supportsInterface 0x01ffc9a7 => true
//@ run-call: supportsInterface 0x80ac58cd => true
//@ run-call: supportsInterface 0x5b5e139f => true
//@ run-call: supportsInterface 0xffffffff => false
//@ run-call: supportsInterface 0x01ffc9a6 => false
//@ run-call: route 0x80ac58cd00000000000000000000000000000000000000000000000000000000 => 1
//@ run-call: route 0x5b5e139f00000000000000000000000000000000000000000000000000000000 => 2
//@ run-call: route 0x5b5e139f00000000000000000000000000000000000000000000000000000001 => 0
//@ run-call: route 0x80ac58cdffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0

// Selector checks compare the top 32 bits of a `bytes4` or a calldata word.
contract ComparedMask {
    function supportsInterface(bytes4 interfaceId) external pure returns (bool) {
        return interfaceId == 0x01ffc9a7 || interfaceId == 0x80ac58cd || interfaceId == 0x5b5e139f;
    }

    function route(bytes32 word) external pure returns (uint256) {
        bytes4 selector = bytes4(word);
        if (selector == 0x80ac58cd && uint224(uint256(word)) == 0) return 1;
        if (selector == 0x5b5e139f && uint224(uint256(word)) == 0) return 2;
        return 0;
    }
}
