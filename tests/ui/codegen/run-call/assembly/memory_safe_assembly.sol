//@ codegen-matrix: standard
//@ run-call: bump 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe338
//@ run-call: bump 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe33b
//@ run-call: bump 5, 3 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe331
//@ run-call: throughLoaded 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe37f
//@ run-call: throughLoaded 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe37f
//@ run-call: throughLoaded 5, 3 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe37f
//@ run-call: legacyComment 5, 0 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe331
//@ run-call: legacyComment 5, 1 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe331
//@ run-call: legacyComment 5, 3 => 0x4c674117131cbcdbf0c0559124f7d7913d38c92629feab3638574c9e053fe331
// Assembly that promises memory safety touches only allocated memory, so the compiler trusts the
// references that memory objects hold and keeps its spills in memory across such code, which a
// module with other assembly does not: each function holds twenty-four values across the writes,
// bumping the free memory pointer, or writing through words loaded from objects, as the assembly
// marked by the `memory-safe` dialect flag or by the `@solidity memory-safe-assembly` comment does.
contract MemorySafeAssembly {
    function bump(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        uint256 word;
        assembly ("memory-safe") {
            let p := mload(0x40)
            mstore(0x40, add(p, 0x60))
            mstore(p, flag)
            mstore(add(p, 0x20), mul(flag, 2))
            word := add(mload(p), mload(add(p, 0x20)))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(word);
    }

    function throughLoaded(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        uint256[] memory xs = new uint256[](4);
        bytes memory b = new bytes(0x40);
        assembly ("memory-safe") {
            let p := add(xs, 0x20)
            mstore(add(p, mul(flag, 0x20)), 7)
            mstore(add(b, 0x20), mload(add(p, mul(flag, 0x20))))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(xs[flag]) ^ bytes32(b.length);
    }

    function legacyComment(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes32 h17 = keccak256(abi.encodePacked(a, uint256(17)));
        bytes32 h18 = keccak256(abi.encodePacked(a, uint256(18)));
        bytes32 h19 = keccak256(abi.encodePacked(a, uint256(19)));
        bytes32 h20 = keccak256(abi.encodePacked(a, uint256(20)));
        bytes32 h21 = keccak256(abi.encodePacked(a, uint256(21)));
        bytes32 h22 = keccak256(abi.encodePacked(a, uint256(22)));
        bytes32 h23 = keccak256(abi.encodePacked(a, uint256(23)));
        uint256[] memory xs = new uint256[](4);
        /// @solidity memory-safe-assembly
        assembly {
            mstore(add(add(xs, 0x20), mul(flag, 0x20)), 9)
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(xs[flag]);
    }
}
