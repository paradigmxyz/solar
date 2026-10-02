//@ codegen-matrix: standard
//@ run-call: normalizedReturn 0 => false
//@ run-call: normalizedReturn 2 => true
//@ run-call: compareRawReturn 2, true => true
//@ run-call: compareRawReturn 2, false => false
//@ run-call: compareReturns 0, 0 => true
//@ run-call: compareReturns 0, 1 => false
//@ run-call: compareReturns 2, 3 => true
//@ run-call: clean 0x0000000000000000000000000000000000000123 => 291
//@ run-call: cleanIsZero 0x0000000000000000000000000000000000000000 => true
//@ run-call: cleanIsZero 0x0000000000000000000000000000000000000001 => false
//@ run-call: dirtyIsZero 0x10000000000000000000000000000000000000000 => true
//@ run-call: dirtyIsZero 0x10000000000000000000000000000000000000001 => false
//@ run-call: cleanCast 0x0000000000000000000000000000000000000123 => 291
//@ run-call: dirtyCast 0x10000000000000000000000000000000000000001 => 1
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

//@ run-call: selectedClean false => 7
//@ run-call: selectedClean true => 19
//@ run-call: selectedDirty 0 => 7
//@ run-call: selectedDirty 2 => 19
//@ run-call: selectedDirty 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 19

contract DirtyArguments {
    function normalizedReturn(uint256 word) external pure returns (bool) {
        return rawReturn(word);
    }

    function compareRawReturn(uint256 word, bool flag) external pure returns (bool) {
        return rawReturn(word) == flag;
    }

    function rawReturn(uint256 word) internal pure returns (bool result) {
        assembly { result := word }
    }

    function compareReturns(uint256 a, uint256 b) external pure returns (bool) {
        return canonicalReturn(a) == canonicalReturn(b);
    }

    function canonicalReturn(uint256 word) internal pure returns (bool) {
        if (word == 0) return true;
        return false;
    }

    function selectedClean(bool condition) external pure returns (uint256) {
        return selected(condition);
    }

    function selectedDirty(uint256 word) external pure returns (uint256) {
        bool condition;
        assembly { condition := word }
        return selected(condition);
    }

    function selected(bool condition) internal pure returns (uint256) {
        return condition ? 19 : 7;
    }

    function cleanIsZero(address value) external pure returns (bool) {
        return value == address(0);
    }

    function dirtyIsZero(uint256 word) external pure returns (bool) {
        address value;
        assembly { value := word }
        return value == address(0);
    }

    function cleanCast(address value) external pure returns (uint256) {
        return uint256(uint160(value));
    }

    function dirtyCast(uint256 word) external pure returns (uint256) {
        address value;
        assembly { value := word }
        return uint256(uint160(value));
    }

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
