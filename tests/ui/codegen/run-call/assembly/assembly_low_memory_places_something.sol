//@ codegen-matrix: standard
//@ run-call: bareLayout 5, 40 => true
//@ run-call: spilledLayout 5, 40 => 0x6ae76ef32a2e583db4df1f208a32cb3bba18e72c5e466a519d26ab93240a6e36, true
//@ run-call: framedLayout 5, 40 => 200, true

// An entry that lays memory out at addresses calldata sizes still moves what the compiler places
// for it above that memory: `spilledLayout` a spill area, and `framedLayout` the frame of a
// function it calls, which writes where the layout reads back unless it lies above it. The route
// that places nothing keeps its heap in low memory beside them.
contract AssemblyLowMemoryPlacesSomething {
    function bareLayout(uint256 a, uint256 words) external pure returns (bool lowHeap) {
        _lay(a);
        words;
        assembly {
            let p := mload(0x40)
            lowHeap := and(lt(p, 0x2000), iszero(mload(sub(p, 0x20))))
        }
    }

    function spilledLayout(uint256 a, uint256 words)
        external
        pure
        returns (bytes32 result, bool raised)
    {
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
        result = _lay(a);
        words;
        assembly {
            raised := iszero(lt(mload(0x40), 0x2080))
        }
        result ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }

    function framedLayout(uint256 a, uint256 words)
        external
        pure
        returns (uint256 sum, bool raised)
    {
        _lay(a);
        bytes32 h = _frame(a);
        words;
        assembly {
            raised := and(iszero(lt(mload(0x40), 0x2080)), iszero(iszero(h)))
            let end := add(0xa0, shl(5, calldataload(0x24)))
            for { let ptr := 0xa0 } lt(ptr, end) { ptr := add(ptr, 0x20) } {
                sum := add(sum, mload(ptr))
            }
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

    // Keeps its hashes live across each other, so that its frame holds words.
    function _frame(uint256 a) internal pure returns (bytes32 r) {
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
        r = keccak256(abi.encodePacked(h0));
        r ^= keccak256(abi.encodePacked(r));
        r ^= h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16;
    }
}
