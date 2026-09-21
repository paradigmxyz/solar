//@ codegen-matrix: standard
//@ run-call: AllocatingCopy::check 0x112233, 7 => 25
//@ run-call: Harness::run => 1
//@ run-call: HeapPrefixTuple::check 1 => 9
//@ run-call: HeapPrefixTuple::checkSecond 1 => 9
//@ run-call: HeapPrefixConstructor::saved; constructor=[7] => 7
//@ run-call: HeapPrefixConstructorHelper::saved; constructor=[7] => 7
//@ run-call: HeapPrefixRecursive::check 7, 3 => 10

// Hand-written creation-code builders may temporarily use memory immediately
// before a heap object and restore it after `create2`. Static internal frames
// end exactly where the heap begins, so spilling the saved words into that
// prefix lets the image overwrite its own restoration state. Values live
// across a backward heap write must remain on the stack until the image is no
// longer live.

contract Implementation {
    function ping() external pure returns (uint256) {
        return 7;
    }
}

contract Harness {
    function run() external returns (uint256) {
        Implementation implementation = new Implementation();
        bytes memory data = hex"112233445566778899aabbccddeeff";
        address instance = _cloneDeterministic(address(implementation), data, bytes32(uint256(1)));
        require(Implementation(instance).ping() == 7, "proxy");
        return 1;
    }

    function _cloneDeterministic(address implementation, bytes memory data, bytes32 salt)
        internal
        returns (address instance)
    {
        uint256 creationStart = _creationStart(data);
        assembly {
            let mBefore3 := mload(sub(data, 0x60))
            let mBefore2 := mload(sub(data, 0x40))
            let mBefore1 := mload(sub(data, 0x20))
            let dataLength := mload(data)
            let dataEnd := add(add(data, 0x20), dataLength)
            let mAfter1 := mload(dataEnd)
            let extraLength := add(dataLength, 2)

            mstore(data, 0x5af43d3d93803e606057fd5bf3)
            mstore(sub(data, 0x0d), implementation)
            mstore(
                sub(data, 0x21),
                or(shl(0x48, extraLength), 0x593da1005b363d3d373d3d3d3d610000806062363936013d73)
            )
            mstore(
                sub(data, 0x3a),
                0x9e4ac34f21c619cefc926c8bd93b54bf5a39c7ab2127a895af1cc0691d7e3dff
            )
            mstore(
                sub(data, add(0x59, lt(extraLength, 0xff9e))),
                or(shl(0x78, add(extraLength, 0x62)), 0xfd6100003d81600a3d39f336602c57343d527f)
            )
            mstore(dataEnd, shl(0xf0, extraLength))

            instance := create2(0, creationStart, add(extraLength, 0x6c), salt)
            if iszero(instance) { revert(0, 0) }

            mstore(dataEnd, mAfter1)
            mstore(data, dataLength)
            mstore(sub(data, 0x20), mBefore1)
            mstore(sub(data, 0x40), mBefore2)
            mstore(sub(data, 0x60), mBefore3)
        }
    }

    function _creationStart(bytes memory data) internal pure returns (uint256 start) {
        assembly {
            start := sub(data, 0x4c)
        }
    }
}

// A tuple-returned pointer must reserve the heap prefix even when it is a raw
// integer. Keep a value live across the helper call and the backward write.
contract HeapPrefixTuple {
    function check(uint256 seed) external pure returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := sub(mload(0x40), 160)
                second := 7
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore(first, 999)
            result := add(live, second)
        }
    }

    function checkSecond(uint256 seed) external pure returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := 7
                second := sub(mload(0x40), 128)
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore(second, 999)
            result := add(live, first)
        }
    }
}

contract HeapPrefixConstructor {
    uint256 public saved;

    constructor(uint256 seed) {
        bytes memory data = new bytes(32);
        assembly {
            mstore(sub(data, 32), 0xdeadbeef)
        }
        saved = seed;
    }
}

contract HeapPrefixConstructorHelper {
    uint256 public saved;

    constructor(uint256 seed) {
        saved = build(seed);
    }

    function build(uint256 seed) internal pure returns (uint256) {
        bytes memory data = new bytes(32);
        assembly {
            mstore(sub(data, 160), 0xdeadbeef)
        }
        return seed;
    }
}

contract HeapPrefixRecursive {
    function check(uint256 seed, uint256 depth) external pure returns (uint256) {
        return build(seed, depth);
    }

    function build(uint256 seed, uint256 depth) internal pure returns (uint256) {
        if (depth != 0) return build(seed, depth - 1) + 1;
        bytes memory data = new bytes(32);
        assembly {
            mstore(sub(data, 288), 0xdeadbeef)
        }
        return seed;
    }
}

contract AllocatingCopy {
    function check(bytes calldata data, uint256 seed) external pure returns (uint256) {
        uint256 live = increment(seed);
        uint256 ptr = allocate(data.length);
        assembly ("memory-safe") {
            calldatacopy(ptr, data.offset, data.length)
        }
        if (data.length == 0) return live;
        uint256 first;
        assembly ("memory-safe") {
            first := byte(0, mload(ptr))
        }
        return live + first;
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function allocate(uint256 size) internal pure returns (uint256 ptr) {
        assembly ("memory-safe") {
            ptr := mload(0x40)
            let end := add(ptr, and(add(size, 31), not(31)))
            if or(gt(end, 0xffffffffffffffff), lt(end, ptr)) { revert(0, 0) }
            mstore(0x40, end)
        }
    }
}
