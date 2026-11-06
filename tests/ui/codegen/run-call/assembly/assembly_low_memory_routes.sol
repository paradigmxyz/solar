//@ codegen-matrix: standard
//@ run-call: layout 5, 40; gas=1000000 => 0x6ae76ef32a2e583db4df1f208a32cb3bba18e72c5e466a519d26ab93240a6e36
//@ run-call: lowHeapRoute 5 => true

// One route lays memory out at addresses that calldata sizes, which moves its spill area and the
// frames it reaches above low memory. Other routes keep theirs: the dispatcher's tail calls never
// return, so it does not take that route's bound, and a route that only reads a far address does
// not move the layout route's memory there.
contract AssemblyLowMemoryRoutes {
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

    function lowHeapRoute(uint256 a) external pure returns (bool lowHeap) {
        bytes32 h = _spillHashes(a);
        assembly {
            lowHeap := and(lt(mload(0x40), 0x2000), iszero(iszero(h)))
        }
    }

    function highRead() external pure returns (uint256 word) {
        assembly {
            word := mload(0x100000)
        }
    }

    function _spillHashes(uint256 a) internal pure returns (bytes32) {
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
        return h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14
            ^ h15 ^ h16;
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
}
