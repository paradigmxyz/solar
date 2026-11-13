//@ codegen-matrix: standard
//@ run-call: bareLayout 5, 40 => true
//@ run-call: framedRoute 5 => true

// An entry that lays memory out at addresses calldata sizes owns the low memory up to `0x2080`,
// but only what the compiler places for that entry has to stay clear of it: a spill area, static
// allocations, and the frames of the functions it reaches. `bareLayout` places none of them; only
// its read of the word before the heap, which reserves a heap prefix, used to raise it. Its heap
// now stays in low memory, and it no longer raises the region that `framedRoute`'s frame starts
// at.
contract AssemblyLowMemoryPlacesNothing {
    function bareLayout(uint256 a, uint256 words) external pure returns (bool lowHeap) {
        _lay(a);
        words;
        assembly {
            let p := mload(0x40)
            lowHeap := and(lt(p, 0x2000), iszero(mload(sub(p, 0x20))))
        }
    }

    function framedRoute(uint256 a) external pure returns (bool lowHeap) {
        bytes32 h = _frame(a);
        assembly {
            lowHeap := and(lt(mload(0x40), 0x2000), iszero(iszero(h)))
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
