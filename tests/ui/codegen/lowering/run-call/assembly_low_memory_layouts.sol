//@ codegen-matrix: standard
//@ run-call: resetHeap 5, 1 => 0xb5a11adf3f4c545585de66ee565ba0cab1cbedf12ce19015194e5a2320dec356
//@ run-call: layout 5, 40 => 0x6ae76ef32a2e583db4df1f208a32cb3bba18e72c5e466a519d26ab93240a6e36
//@ run-call-fail: revertWithTime 6, 7 => 0x21ccfeb700000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000007
//@ run-call: hashScratch 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c, 5
//@ run-call: hashInHelper 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: storeInHelper 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: deepAddress 5 => 5
//@ run-call: seal => 0xa0335e2fb93a29d2fa9e4b93eeb7ecc9a9ed34740c749d416b560fad02be2ab5
//@ run-call: resetLoop 1, 32 => 0xf937df1b2e7b71cce0c407a18a6afc64603df82e7adbc88898f1269c62f6d596
//@ run-call: hashRecursive 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: hashThenCopy 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: readScratch 5 => 5
//@ run-call: readInHelper 5 => 5
//@ run-call: hashAt 0x40, 3 => 0xc2575a0e9e593c00f959f8c92f12db2869c3395a3b0502d05e2516446f71f85b
//@ run-call: zeroLengthCopy 5 => true
//@ run-call: mixedLoad 5 => 5
//@ run-call: dataThenMixed 5 => 5, 5
//@ run-call: tupleReturn 5 => 5, 7
//@ run-call: tupleThroughHelper 5 => 5, 7
//@ run-call: tupleThroughYul 5 => 5, 7
//@ run-call: tupleReturnPublic 5 => 5, 7
//@ run-call: tupleThroughPublic 5 => 5, 7
//@ run-call: checkedScaled 0 => 0
//@ run-call: hashHelperScratch 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: allocateAfterHelper 1, 2, 3 => 3
//@ run-call: hashAroundCheckedAdd 1, 2, 3 => 0x6e0c627900b24bd432fe7b1f713f1b0744091a646a9fe4a65a18dfed21f2949c
//@ run-call: layoutAbove 5, 40 => 0x6ae76ef32a2e583db4df1f208a32cb3bba18e72c5e466a519d26ab93240a6e36
//@ run-call: bufferAboveHeap 5, 0 => 1029

// Memory-unsafe assembly can treat all memory from 0x80 up as its own. Seaport lays a basic
// order's hashes and event data out at addresses that calldata sizes, moves the free memory
// pointer just past them, and resets it to 0x80 after batch transfers. The backend keeps
// spill slots and internal-call frames below the initial free memory pointer, so the next
// allocation overwrote live words: a decoded signature replaced the offerer and the order
// hash, and the event data replaced the offered item type. A store that would lower the
// free memory pointer now keeps it at or above the initial one, and a layout sized by
// calldata moves the spill area above low memory, and above the fixed memory the assembly names
// there. Storing calldata into the pointer's slot as
// an error argument or a hash input keeps its value, also across calls to assembly helpers and
// checked arithmetic, and so does a load that reads the slot back as data. A caller that
// allocates from a helper's scratch word clamps it on its own path.
contract AssemblyLowMemoryLayouts {
    bytes32 public seal;

    // A constructor that hashes data in the pointer's slot keeps it there: the constructor's
    // return copies the runtime code to a fixed address and never reads the slot again.
    constructor() {
        bytes32 h;
        assembly {
            mstore(0x00, callvalue())
            mstore(0x20, 2)
            mstore(0x40, 3)
            h := keccak256(0x00, 0x60)
        }
        seal = h;
    }

    function resetHeap(uint256 a, uint256 flag) external pure returns (bytes32 result) {
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        assembly {
            mstore(0x40, 0x80)
        }
        if (flag != 0) {
            bytes memory copy = _copy(1024);
            result = bytes32(copy.length);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14
            ^ h15 ^ h16;
    }

    function layout(uint256 a, uint256 words) external pure returns (bytes32 result) {
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        if (words != 0) {
            result = _lay(a);
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14
            ^ h15 ^ h16;
    }

    // A fixed copy above low memory keeps clear of the spills that a calldata-sized layout moves
    // there.
    function layoutAbove(uint256 a, uint256 words) external pure returns (bytes32 result) {
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        if (words != 0) {
            result = _lay(a);
        }
        assembly {
            calldatacopy(0x2080, calldatasize(), 0x400)
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14
            ^ h15 ^ h16;
    }

    // A fixed buffer above low memory keeps clear of the heap that such a layout moves there.
    function bufferAboveHeap(uint256 a, uint256 words) external pure returns (uint256 kept) {
        assembly {
            mstore(0x2400, a)
        }
        if (words != 0) {
            _lay(a);
        }
        bytes memory fresh = new bytes(0x400);
        assembly {
            kept := add(mload(0x2400), mload(fresh))
        }
    }

    // A copy of no bytes to a calldata address checks the returndata bounds but writes nothing, so
    // the spill area and the heap stay in low memory.
    function zeroLengthCopy(uint256) external view returns (bool lowHeap) {
        assembly {
            pop(staticcall(gas(), 4, 0, 1, 0, 0))
            returndatacopy(calldataload(4), 1, 0)
            lowHeap := lt(mload(0x40), 0x2000)
        }
    }

    // The pointer read right after a reset is the clamped one in every build: optimized MIR
    // forwards the stored word to the load, so the clamp is part of the stored word.
    function resetLoop(uint256 a, uint256 n) external pure returns (bytes32 result) {
        bytes32 h0 = keccak256(abi.encodePacked(a, uint256(0)));
        bytes32 h1 = keccak256(abi.encodePacked(a, uint256(1)));
        bytes32 h2 = keccak256(abi.encodePacked(a, uint256(2)));
        bytes32 h3 = keccak256(abi.encodePacked(a, uint256(3)));
        bytes32 h4 = keccak256(abi.encodePacked(a, uint256(4)));
        bytes32 h5 = keccak256(abi.encodePacked(a, uint256(5)));
        bytes32 h6 = keccak256(abi.encodePacked(a, uint256(6)));
        bytes32 h7 = keccak256(abi.encodePacked(a, uint256(7)));
        bytes32 h8 = keccak256(abi.encodePacked(a, uint256(8)));
        bytes32 h9 = keccak256(abi.encodePacked(a, uint256(9)));
        bytes32 h10 = keccak256(abi.encodePacked(a, uint256(10)));
        bytes32 h11 = keccak256(abi.encodePacked(a, uint256(11)));
        bytes32 h12 = keccak256(abi.encodePacked(a, uint256(12)));
        bytes32 h13 = keccak256(abi.encodePacked(a, uint256(13)));
        bytes32 h14 = keccak256(abi.encodePacked(a, uint256(14)));
        bytes32 h15 = keccak256(abi.encodePacked(a, uint256(15)));
        bytes32 h16 = keccak256(abi.encodePacked(a, uint256(16)));
        assembly {
            mstore(0x40, 0x80)
            let p := mload(0x40)
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                mstore(add(p, shl(5, i)), not(0))
            }
            mstore(0x40, add(p, shl(5, n)))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14
            ^ h15 ^ h16 ^ bytes32(n);
    }

    function revertWithTime(uint256 startTime, uint256) external pure {
        assembly {
            mstore(0, 0x21ccfeb7) // `InvalidTime(uint256,uint256)`
            mstore(0x20, startTime)
            mstore(0x40, calldataload(0x24)) // `endTime`.
            revert(0x1c, 0x44)
        }
    }

    function hashScratch(uint256 a, uint256 b, uint256)
        external
        pure
        returns (bytes32 h, uint256 length)
    {
        assembly {
            let m := mload(0x40)
            mstore(0x00, a)
            mstore(0x20, b)
            mstore(0x40, calldataload(0x44)) // `c`.
            h := keccak256(0x00, 0x60)
            mstore(0x40, m)
        }
        bytes memory fresh = new bytes(5);
        length = fresh.length;
    }

    function hashInHelper(uint256 a, uint256 b, uint256) external pure returns (bytes32 h) {
        assembly {
            function hashScratch() -> r {
                r := keccak256(0x00, 0x60)
            }
            let m := mload(0x40)
            mstore(0x00, a)
            mstore(0x20, b)
            mstore(0x40, calldataload(0x44)) // `c`.
            h := hashScratch()
            mstore(0x40, m)
        }
    }

    function storeInHelper(uint256 a, uint256 b, uint256) external pure returns (bytes32 h) {
        assembly {
            function put(x, y) {
                mstore(0x00, x)
                mstore(0x20, y)
                mstore(0x40, calldataload(0x44)) // `c`.
            }
            let m := mload(0x40)
            put(a, b)
            h := keccak256(0x00, 0x60)
            mstore(0x40, m)
        }
    }

    // A destination built from many rounds over shared words is classified once per word.
    function deepAddress(uint256) external pure returns (uint256 r) {
        assembly {
            let x := calldataload(4)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            x := add(x, x)
            mstore(and(x, 0x1f), calldataload(4))
            r := mload(0)
        }
    }

    function hashRecursive(uint256 a, uint256 b, uint256) external pure returns (bytes32 h) {
        assembly {
            function hashFrom(depth) -> r {
                switch depth
                case 0 { r := keccak256(0x00, 0x60) }
                default { r := hashFrom(sub(depth, 1)) }
            }
            let m := mload(0x40)
            mstore(0x00, a)
            mstore(0x20, b)
            mstore(0x40, calldataload(0x44)) // `c`.
            h := hashFrom(0)
            mstore(0x40, m)
        }
    }

    // A copy over the whole slot replaces the hashed word before the slot is read as a pointer.
    function hashThenCopy(uint256 a, uint256 b, uint256) external pure returns (bytes32 h) {
        assembly {
            mstore(0x00, a)
            mstore(0x20, b)
            mstore(0x40, calldataload(0x44)) // `c`.
            h := keccak256(0x00, 0x60)
            calldatacopy(0x40, 0x24, 0x20) // `b`.
            let p := mload(0x40)
            mstore(p, h)
        }
    }

    function readScratch(uint256) external pure returns (uint256 read) {
        assembly {
            let m := mload(0x40)
            mstore(0x40, calldataload(4))
            read := mload(0x40)
            mstore(0x40, m)
        }
    }

    function readInHelper(uint256) external pure returns (uint256 read) {
        assembly {
            function readWord() -> word {
                let m := mload(0x40)
                mstore(0x40, calldataload(4))
                word := mload(0x40)
                mstore(0x40, m)
            }
            read := readWord()
        }
    }

    // A hash at an offset the caller supplies can read the scratch word.
    function hashAt(uint256 offset, uint256) external pure returns (bytes32 h) {
        assembly {
            mstore(0x40, calldataload(0x24))
            h := keccak256(offset, 0x20)
            let p := mload(0x40)
            mstore(p, h)
        }
    }

    // A load whose word is both written through and returned: only the write is clamped.
    function mixedLoad(uint256) external pure returns (uint256 seen) {
        assembly {
            mstore(0x40, calldataload(4))
            let p := mload(0x40)
            mstore(p, 1)
            seen := p
            mstore(0x40, add(p, 0x20))
        }
    }

    function dataThenMixed(uint256) external pure returns (uint256 first, uint256 seen) {
        assembly {
            mstore(0x40, calldataload(4))
            first := mload(0x40)
            let p := mload(0x40)
            mstore(p, 1)
            seen := p
        }
    }

    function tupleReturn(uint256) external pure returns (uint256 read, uint256 other) {
        assembly {
            let m := mload(0x40)
            mstore(0x40, calldataload(4))
            read := mload(0x40)
            mstore(0x40, m)
            other := 7
        }
    }

    // A pair of integers returned through an internal, Yul, or public helper is data as well.
    function tupleThroughHelper(uint256) external pure returns (uint256, uint256) {
        return _scratchPair();
    }

    function tupleThroughYul(uint256) external pure returns (uint256 read, uint256 other) {
        assembly {
            function scratchPair() -> a, b {
                let m := mload(0x40)
                mstore(0x40, calldataload(4))
                a := mload(0x40)
                mstore(0x40, m)
                b := 7
            }
            read, other := scratchPair()
        }
    }

    function tupleReturnPublic(uint256) public pure returns (uint256 read, uint256 other) {
        assembly {
            let m := mload(0x40)
            mstore(0x40, calldataload(4))
            read := mload(0x40)
            mstore(0x40, m)
            other := 7
        }
    }

    function tupleThroughPublic(uint256 value) external pure returns (uint256, uint256) {
        return tupleReturnPublic(value);
    }

    // A checked product on the way to a write keeps its check on the loaded word; the clamped
    // address it writes through wraps instead.
    function checkedScaled(uint256) external pure returns (uint256 seen) {
        uint256 p;
        assembly {
            mstore(0x40, calldataload(4))
            p := mload(0x40)
        }
        uint256 q = p * (1 << 255);
        assembly {
            mstore(q, 1)
        }
        seen = p;
    }

    function hashHelperScratch(uint256 a, uint256 b, uint256) external pure returns (bytes32 h) {
        uint256 m;
        assembly {
            m := mload(0x40)
        }
        _putScratch();
        assembly {
            mstore(0x00, a)
            mstore(0x20, b)
            h := keccak256(0x00, 0x60)
            mstore(0x40, m)
        }
    }

    function allocateAfterHelper(uint256, uint256, uint256 c) external pure returns (uint256) {
        _putScratch();
        bytes memory fresh = new bytes(c);
        return fresh.length;
    }

    // A checked addition may revert, but it reverts with scratch data and never reads the slot.
    function hashAroundCheckedAdd(uint256 a, uint256 b, uint256)
        external
        pure
        returns (bytes32 h)
    {
        uint256 m;
        assembly {
            m := mload(0x40)
            mstore(0x40, calldataload(0x44)) // `c`.
        }
        uint256 sum = a + b;
        assembly {
            mstore(0x00, a)
            mstore(0x20, b)
            h := keccak256(0x00, 0x60)
            mstore(0x40, m)
        }
        h ^= bytes32(sum - a - b);
    }

    function _putScratch() internal pure {
        assembly {
            mstore(0x40, calldataload(0x44)) // The third argument.
        }
    }

    function _copy(uint256 n) internal pure returns (bytes memory out) {
        assembly {
            out := mload(0x40)
            mstore(out, n)
            codecopy(add(out, 0x20), 0, n)
            mstore(0x40, add(add(out, 0x20), n))
        }
    }

    // Fills words from 0xa0 up to an end that calldata sizes, like Seaport's event data.
    function _lay(uint256 a) internal pure returns (bytes32 h) {
        assembly {
            let end := add(0xa0, shl(5, calldataload(0x24)))
            for { let ptr := 0xa0 } lt(ptr, end) { ptr := add(ptr, 0x20) } {
                mstore(ptr, a)
            }
            mstore(end, a)
            h := keccak256(0xa0, add(sub(end, 0xa0), 0x20))
        }
    }

    function _scratchPair() internal pure returns (uint256 read, uint256 other) {
        assembly {
            let m := mload(0x40)
            mstore(0x40, calldataload(4))
            read := mload(0x40)
            mstore(0x40, m)
            other := 7
        }
    }
}
