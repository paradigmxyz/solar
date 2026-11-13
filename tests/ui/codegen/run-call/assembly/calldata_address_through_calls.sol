//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: yulHelper 5, 0x80 => 5
//@ run-call: internalHelper 5, 0x80 => 5

// Assembly writes at an address that calldata supplies while 17 live hashes do not fit on the
// stack. The address reaches the write as a Yul helper's result, or as a parameter of an internal
// function, and the spilled hashes still stay clear of it at 0x80.
contract CalldataAddressThroughCalls {
    function yulHelper(uint256 a, uint256 p) external pure returns (uint256 result) {
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
        uint256 q;
        assembly {
            function id(x) -> r {
                r := x
            }
            q := id(calldataload(0x24))
        }
        assembly {
            for { let i := 0 } lt(i, 1) { i := add(i, 1) } {
                mstore(q, a)
            }
            result := mload(q)
        }
        result ^= uint256(h0 ^ h0 ^ h1 ^ h1 ^ h2 ^ h2 ^ h3 ^ h3 ^ h4 ^ h4 ^ h5 ^ h5 ^ h6 ^ h6 ^ h7 ^ h7 ^ h8 ^ h8 ^ h9 ^ h9 ^ h10 ^ h10 ^ h11 ^ h11 ^ h12 ^ h12 ^ h13 ^ h13 ^ h14 ^ h14 ^ h15 ^ h15 ^ h16 ^ h16);
    }

    function writeWord(uint256 q, uint256 a) internal pure {
        assembly {
            mstore(q, a)
        }
    }

    function readWord(uint256 q) internal pure returns (uint256 result) {
        assembly {
            result := mload(q)
        }
    }

    function internalHelper(uint256 a, uint256 p) external pure returns (uint256 result) {
        writeWord(p, a);
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
        result = readWord(p);
        result ^= uint256(h0 ^ h0 ^ h1 ^ h1 ^ h2 ^ h2 ^ h3 ^ h3 ^ h4 ^ h4 ^ h5 ^ h5 ^ h6 ^ h6 ^ h7 ^ h7 ^ h8 ^ h8 ^ h9 ^ h9 ^ h10 ^ h10 ^ h11 ^ h11 ^ h12 ^ h12 ^ h13 ^ h13 ^ h14 ^ h14 ^ h15 ^ h15 ^ h16 ^ h16);
    }
}
