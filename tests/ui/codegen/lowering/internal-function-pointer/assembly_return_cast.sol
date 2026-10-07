//@ filecheck:
//@ codegen-matrix: standard
//@ run-call: decodePair 1, 2 => 1, 2
//@ run-call: decodePairLater 3, 4 => 34
//@ run-call: decodePairField 5, 6 => 5, 6
//@ run-call: decodeBytes 0x00abff => 0x00abff
//@ run-call: sumPair 3, 4 => 7
//@ run-call: swapArrays [1, 2], [3] => 2, [3], [1, 2]
//@ run-call: applyPair 3, 4 => 12
//@ run-call: isOdd 3 => true
//@ run-call-fail: callInvalid => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051

type CalldataPointer is uint256;
type MemoryPointer is uint256;

struct Pair {
    uint256 a;
    uint256 b;
}

struct WordDecoder {
    function(CalldataPointer) internal pure returns (MemoryPointer) decode;
}

struct PairDecoder {
    function(CalldataPointer) internal pure returns (Pair memory) decode;
}

// Assembly retypes these pointers, as Seaport does, so their calls reach every function whose
// pointer flows into assembly and reinterpret the one-word arguments and results.
contract InternalFunctionPointerAssemblyReturnCast {
    // CHECK-LABEL: fn @decodePair(
    // CHECK: icall @_toPairReturnType, [[DECODE_PAIR:[0-9]+]]
    // CHECK: icall @internal_dispatcher_asm_p_u256_r_memorystruct,
    function decodePair(uint256, uint256) external pure returns (uint256, uint256) {
        Pair memory pair = _toPairReturnType(_decodePair)(CalldataPointer.wrap(4));
        return (pair.a, pair.b);
    }

    function decodePairLater(uint256, uint256) external pure returns (uint256) {
        function(CalldataPointer) internal pure returns (Pair memory) decode =
            _toPairReturnType(_decodePair);
        Pair memory pair = decode(CalldataPointer.wrap(4));
        return pair.a * 10 + pair.b;
    }

    function decodePairField(uint256, uint256) external pure returns (uint256, uint256) {
        Pair memory pair =
            _toPairDecoder(WordDecoder(_decodePair)).decode(CalldataPointer.wrap(4));
        return (pair.a, pair.b);
    }

    function decodeBytes(bytes calldata) external pure returns (bytes memory) {
        return _toBytesReturnType(_decodeBytes)(CalldataPointer.wrap(0x24));
    }

    function sumPair(uint256 a, uint256 b) external pure returns (uint256) {
        return _toPairInput(_sumWords)(Pair(a, b));
    }

    function swapArrays(uint256[] memory left, uint256[] memory right)
        external
        pure
        returns (uint256, uint256[] memory, uint256[] memory)
    {
        return _toArrayTypes(_swapWords)(left, right);
    }

    // A typed callback passed through a retyped call reaches the generic helper's word-typed
    // parameter, as with Seaport's `ArrayHelpers.mapWithArg`.
    function applyPair(uint256 a, uint256 b) external pure returns (uint256) {
        return _toPairApply(_applyWord)(Pair(a, b), _productOf);
    }

    // CHECK-LABEL: fn @isOdd(
    // CHECK: icall @internal_dispatcher_p_u256_r_bool,
    function isOdd(uint256 value) external pure returns (bool) {
        function(uint256) internal pure returns (bool) predicate = _isOdd;
        return predicate(value);
    }

    function callInvalid() external pure returns (uint256) {
        function(MemoryPointer) internal pure returns (uint256) sum;
        assembly {
            sum := 0x1234
        }
        return sum(MemoryPointer.wrap(0));
    }

    function _decodePair(CalldataPointer cdPtr) internal pure returns (MemoryPointer mPtr) {
        assembly {
            mPtr := mload(0x40)
            mstore(mPtr, calldataload(cdPtr))
            mstore(add(mPtr, 0x20), calldataload(add(cdPtr, 0x20)))
            mstore(0x40, add(mPtr, 0x40))
        }
    }

    function _decodeBytes(CalldataPointer cdPtrLength)
        internal
        pure
        returns (MemoryPointer mPtrLength)
    {
        assembly {
            mPtrLength := mload(0x40)
            let size := add(and(add(calldataload(cdPtrLength), 31), not(31)), 32)
            calldatacopy(mPtrLength, cdPtrLength, size)
            mstore(0x40, add(mPtrLength, size))
        }
    }

    function _sumWords(MemoryPointer mPtr) internal pure returns (uint256 sum) {
        assembly {
            sum := add(mload(mPtr), mload(add(mPtr, 0x20)))
        }
    }

    function _swapWords(MemoryPointer first, MemoryPointer second)
        internal
        pure
        returns (uint256 count, MemoryPointer left, MemoryPointer right)
    {
        return (2, second, first);
    }

    function _applyWord(MemoryPointer value, function(MemoryPointer) internal pure returns (uint256) fn)
        internal
        pure
        returns (uint256)
    {
        return fn(value);
    }

    function _productOf(Pair memory pair) internal pure returns (uint256) {
        return pair.a * pair.b;
    }

    function _isOdd(uint256 value) internal pure returns (bool) {
        return value % 2 == 1;
    }

    // A pointer that comes from assembly can hold any exposed function with a one-word shape;
    // the dispatcher reinterprets the `MemoryPointer` word as the `memory` struct reference.
    // CHECK-LABEL: fn @internal_dispatcher_asm_p_u256_r_memorystruct(
    // CHECK: eq arg0, [[DECODE_PAIR]]
    // CHECK: [[WORD:v[0-9]+]] = icall @_decodePair, arg1
    // CHECK-NEXT: inttoptr i256 [[WORD]] to memptr
    //
    // A pointer that never touches assembly keeps the per-type dispatch.
    // CHECK-LABEL: fn @internal_dispatcher_p_u256_r_bool(
    // CHECK-NOT: @_decodePair
    // CHECK: icall @_isOdd
    // CHECK-NOT: @_decodePair
    // CHECK-LABEL: fn @internal_dispatcher_asm_p_u256_r_u256(
    function _toPairReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    ) internal pure returns (function(CalldataPointer) internal pure returns (Pair memory) outFn) {
        assembly {
            outFn := inFn
        }
    }

    function _toPairDecoder(WordDecoder memory inDecoder)
        internal
        pure
        returns (PairDecoder memory outDecoder)
    {
        assembly {
            outDecoder := inDecoder
        }
    }

    function _toBytesReturnType(
        function(CalldataPointer) internal pure returns (MemoryPointer) inFn
    ) internal pure returns (function(CalldataPointer) internal pure returns (bytes memory) outFn) {
        assembly {
            outFn := inFn
        }
    }

    function _toPairInput(function(MemoryPointer) internal pure returns (uint256) inFn)
        internal
        pure
        returns (function(Pair memory) internal pure returns (uint256) outFn)
    {
        assembly {
            outFn := inFn
        }
    }

    function _toPairApply(
        function(MemoryPointer, function(MemoryPointer) internal pure returns (uint256)) internal pure returns (uint256)
            inFn
    )
        internal
        pure
        returns (
            function(Pair memory, function(Pair memory) internal pure returns (uint256)) internal pure returns (uint256)
                outFn
        )
    {
        assembly {
            outFn := inFn
        }
    }

    function _toArrayTypes(
        function(MemoryPointer, MemoryPointer) internal pure returns (uint256, MemoryPointer, MemoryPointer)
            inFn
    )
        internal
        pure
        returns (
            function(uint256[] memory, uint256[] memory) internal pure returns (uint256, uint256[] memory, uint256[] memory)
                outFn
        )
    {
        assembly {
            outFn := inFn
        }
    }
}
