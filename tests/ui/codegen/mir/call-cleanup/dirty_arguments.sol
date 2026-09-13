//@ codegen-matrix: standard
//@ run-call: clean 0x0000000000000000000000000000000000000123 => 291
//@ run-call-fail: 0x169b26230000000000000000000000010000000000000000000000000000000000000000
//@ run-call: dirty 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call: dirty 0x10000000000000000000000000000000000000001 => 1
//@ run-call: observeDirty 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: dirtyBool 2 => 0
//@ run-call: dirtyBool 3 => 1
//@ run-call: cleanBool true => 1
//@ run-call: cleanBool false => 0
//@ run-call: narrow 511 => 255
//@ run-call: recurse 511, 3 => 255

contract DirtyArguments {
    function clean(address value) external pure returns (uint256) {
        return addressBits(value);
    }

    function dirty(uint256 word) external pure returns (uint256) {
        address value;
        assembly { value := word }
        return addressBits(value);
    }

    // Both validated and dirty callers reach this function. The nominal
    // address type cannot justify dropping its mask.
    function addressBits(address value) internal pure returns (uint256 result) {
        assembly { result := and(value, 0xffffffffffffffffffffffffffffffffffffffff) }
    }

    function observeDirty(uint256 word) external pure returns (uint256) {
        address value;
        assembly { value := word }
        return rawBits(value);
    }

    function rawBits(address value) internal pure returns (uint256 result) {
        assembly { result := value }
    }

    function cleanBool(bool value) external pure returns (uint256) {
        return boolBit(value);
    }

    function dirtyBool(uint256 word) external pure returns (uint256) {
        bool value;
        assembly { value := word }
        return boolBit(value);
    }

    function boolBit(bool value) internal pure returns (uint256 result) {
        assembly { result := and(value, 1) }
    }

    function narrow(uint256 word) external pure returns (uint256) {
        return forward(uint8(word));
    }

    function forward(uint8 value) internal pure returns (uint256) {
        return uint256(value);
    }

    function recurse(uint256 word, uint256 depth) external pure returns (uint256) {
        require(depth <= 4);
        return recursive(uint8(word), depth);
    }

    function recursive(uint8 value, uint256 depth) internal pure returns (uint256) {
        if (depth == 0) return value;
        return recursive(value, depth - 1);
    }
}
