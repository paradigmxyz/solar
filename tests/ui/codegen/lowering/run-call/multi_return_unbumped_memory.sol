//@ codegen-matrix: standard
//@ run-call: LowFmpAllocation::environmentTuple 41 => 83, true
//@ run-call: LowFmpAllocation::storedTuple 41 => 126
//@ run-call: LowFmpAllocation::discardedSlices 0x1234 => 2
//@ run-call: LowFmpAllocation::discardedTuple 41 => 42
//@ run-call: UnrestrictedScalarConstructor::value; constructor=[41] => 42
//@ run-call: UnrestrictedScalarConstructor::value; constructor=[0] => 1
//@ run-call: IndependentMemoryContexts::output 1 => 17
//@ run-call: IndependentMemoryContexts::recompute 0 => 45
//@ run-call: MemorySafeTupleConstructor::value; constructor=[7] => 1015
//@ run-call: MemorySafeTupleScratch::output 7 => 1015
//@ run-call: LowFmpAllocation::probe 128 => 37, 0
//@ run-call: LowFmpAllocation::computedPointer 96 => 37, 0
//@ run-call: LowFmpAllocation::returnedBuffer => 0
//@ run-call: LowFmpAllocation::returnedBufferHash => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: LowFmpAllocation::returnedBufferSecond => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: LowFmpArguments::output => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: MultiReturnUnbumpedMemory::output => 0x1234000000000000000000000000000000000000000000000000000000000000

contract MultiReturnUnbumpedMemory {
    function output() external pure returns (bytes32 result) {
        assembly {
            function emitWord(cursor) -> next, ignored {
                mstore(cursor, shl(240, 0x1234))
                next := add(cursor, 2)
            }

            let start := add(mload(0x40), 0x20)
            let cursor := start
            let ignored
            cursor, ignored := emitWord(cursor)
            result := mload(start)
        }
    }
}

contract LowFmpAllocation {
    uint128 private packedFirst;
    uint128 private packedSecond;

    function environmentTuple(uint256 value) external view returns (uint256, bool) {
        assembly { mstore(0x40, 0x80) }
        (uint256 first, uint256 second,) = triple(value);
        return (first + second, (block.chainid | uint160(address(this))) != 0);
    }

    function storedTuple(uint128 value) external returns (uint128 result) {
        assembly { mstore(0x40, 0x80) }
        (packedFirst, result,) = smallTriple(value);
        (, uint128 next,) = smallTriple(value + 1);
        return result + next + packedFirst;
    }

    function smallTriple(uint128 value) internal pure returns (uint128, uint128, uint128) {
        return (value, value + 1, value + 2);
    }

    function computedPointer(uint256 base) external pure returns (uint256, uint256) {
        assembly { mstore(0x40, add(base, 32)) }
        return allocate(37, 128);
    }

    function probe(uint256 size) external pure returns (uint256, uint256) {
        assembly { mstore(0x40, 0x80) }
        return allocate(37, size & 255);
    }

    function returnedBuffer() external pure returns (uint256 word) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        assembly { word := mload(add(data, 32)) }
    }

    function returnedBufferHash() external pure returns (bytes32) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        require(increment(41) == 42);
        return keccak256(data);
    }

    function returnedBufferSecond() external pure returns (bytes32) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        (, uint256 second, uint256 third) = triple(41);
        require(second == 42 && third == 43);
        return keccak256(data);
    }

    function discardedSlices(bytes calldata input) external pure returns (uint256) {
        assembly { mstore(0x40, 0x80) }
        slices(input);
        return input.length;
    }

    function slices(bytes calldata input) internal pure returns (bytes calldata, bytes calldata) {
        return (input, input);
    }

    function discardedTuple(uint256 value) external pure returns (uint256 result) {
        assembly { mstore(0x40, 0x80) }
        (, result,) = triple(value);
        triple(value + 1);
    }

    function triple(uint256 value) internal pure returns (uint256, uint256, uint256) {
        return (value, value + 1, value + 2);
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function buffer(uint256 size) internal pure returns (bytes memory) {
        return new bytes(size);
    }

    function allocate(uint256 saved, uint256 size) internal pure returns (uint256, uint256 word) {
        bytes memory data = new bytes(size);
        assembly { word := mload(add(data, 32)) }
        return (saved, word);
    }
}

contract LowFmpArguments {
    function output() external pure returns (bytes32) {
        bytes memory cursor;
        assembly {
            let saved := mload(0x40)
            mstore(0x40, 0x80)
            cursor := mload(0x40)
            mstore(0x40, saved)
        }
        bytes memory data = allocate(cursor);
        return keccak256(data);
    }

    function allocate(bytes memory cursor) internal pure returns (bytes memory) {
        assembly { mstore(0x40, cursor) }
        return new bytes(128);
    }
}

contract MemorySafeTupleScratch {
    function output(uint256 input) external pure returns (uint256 result) {
        assembly ("memory-safe") {
            function pair(value) -> first, second {
                first := value
                second := add(value, 1)
            }
            mstore(0x20, 1000)
            let first, second := pair(input)
            result := add(mload(0x20), add(first, second))
        }
    }
}

contract MemorySafeTupleConstructor {
    uint256 public value;

    constructor(uint256 input) {
        uint256 result;
        /// @solidity memory-safe
        assembly {
            function pair(argument) -> first, second {
                first := argument
                second := add(argument, 1)
            }
            mstore(0x20, 1000)
            let first, second := pair(input)
            result := add(mload(0x20), add(first, second))
        }
        value = result;
    }
}

contract IndependentMemoryContexts {
    function recompute(uint256 input) external pure returns (uint256) {
        unchecked {
            uint256 a = input + 1;
            uint256 b = input + 2;
            uint256 c = input + 3;
            uint256 d = input + 4;
            uint256 e = input + 5;
            uint256 f = input + 6;
            uint256 g = input + 7;
            uint256 h = input + 8;
            uint256 i = input + 9;
            uint256 word;
            assembly { word := mload(input) }
            if (word != 0) return 0;
            return a + b + c + d + e + f + g + h + i;
        }
    }

    function unrestricted() external pure {
        assembly { mstore(0x40, 0x80) }
    }

    function output(uint256 value) external view returns (uint256) {
        uint256 copied;
        assembly {
            if lt(mload(0x40), 0x80) { revert(0, 0) }
            mstore(0, value)
            if iszero(staticcall(gas(), 4, 0, 32, 32, 32)) { revert(0, 0) }
            copied := mload(32)
            if iszero(staticcall(gas(), 4, not(0), 0, not(0), 0)) { revert(0, 0) }
        }
        require(copied == value);
        return sum(value, value, value, value, value, value, value, value, value,
            value, value, value, value, value, value, value, value);
    }

    function sum(
        uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f,
        uint256 g, uint256 h, uint256 i, uint256 j, uint256 k, uint256 l,
        uint256 m, uint256 n, uint256 o, uint256 p, uint256 q
    ) internal pure returns (uint256) {
        unchecked { return a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q; }
    }
}

contract UnrestrictedScalarConstructor {
    uint256 public value;

    constructor(uint256 input) {
        value = forward(input);
    }

    function forward(uint256 input) internal pure returns (uint256) {
        assembly {
            let fmp := mload(0x40)
            codecopy(0, 0, 265)
            mstore(0x40, fmp)
            mstore(0x60, 0)
        }
        return input + 1;
    }
}
