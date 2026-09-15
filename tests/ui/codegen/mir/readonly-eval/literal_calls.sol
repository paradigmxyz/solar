//@ codegen-matrix: standard
//@ run-call: words => 42, 0x3100000000000000000000000000000000000000000000000000000000000001
//@ run-call: dynamicWord 511 => 511
//@ run-call: sideEffects false => 7, 1
//@ run-call: sideEffects true => 42, 0
//@ run-call: modified 511 => 511
//@ run-call-fail: fails

contract LiteralCalls {
    uint256 private touched;
    uint256 private immutable answer;
    bytes32 private immutable shortWord;

    constructor() {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), 41) }
        answer = readWord(data) + 1;
        shortWord = toShort(bytes("1"));
    }

    function words() external view returns (uint256, bytes32) {
        return (answer, shortWord);
    }

    function dynamicWord(uint256 value) external pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), value) }
        return readWord(data);
    }

    function sideEffects(bool yes) external returns (uint256 result, uint256 writes) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), 42) }
        result = choose(data, yes);
        writes = touched;
    }

    function modified(uint256 value) external pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly { mstore(add(data, 32), 42) }
        overwrite(data, value);
        return readWord(data);
    }

    function fails() external pure returns (uint256) {
        return readWord(bytes(""));
    }

    function readWord(bytes memory data) internal pure returns (uint256 result) {
        require(data.length == 32);
        assembly { result := mload(add(data, 32)) }
    }

    function toShort(bytes memory data) internal pure returns (bytes32 result) {
        require(data.length < 32);
        assembly {
            let mask := shl(mul(sub(32, mload(data)), 8), not(0))
            result := or(and(mload(add(data, 32)), mask), mload(data))
        }
    }

    function choose(bytes memory data, bool yes) internal returns (uint256) {
        if (yes) return readWord(data);
        touched += 1;
        return 7;
    }

    function overwrite(bytes memory data, uint256 value) internal pure {
        assembly { mstore(add(data, 32), value) }
    }
}
