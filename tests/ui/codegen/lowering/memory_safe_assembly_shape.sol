//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Spill protection is for modules with assembly that does not promise memory safety, which may
// write to any address. The two contracts differ only in the `memory-safe` flag of the assembly
// block, which stores through a word loaded from memory while holding twenty-four values. The
// memory-safe one keeps them in their spill slots across the store, which touches the last slot
// twice, as does the one marked by the `@solidity memory-safe-assembly` comment. The other saves the spilled words on the stack around the store and restores them
// afterwards, which touches it twice more.
// CHECK-LABEL: @module Safe_runtime
// CHECK-COUNT-2: push 384
// CHECK-NOT: push 384
// CHECK-LABEL: @module SafeComment_runtime
// CHECK-COUNT-2: push 384
// CHECK-NOT: push 384
// CHECK-LABEL: @module Unsafe_runtime
// CHECK-COUNT-4: push 384
// CHECK-NOT: push 384
contract Safe {
    function f(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes memory b = new bytes(0x40);
        uint256[] memory xs = new uint256[](2);
        assembly ("memory-safe") {
            let p := mload(add(xs, 0x20))
            mstore(add(p, mul(flag, 0x20)), 1)
            mstore(add(b, 0x20), mload(xs))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(b.length);
    }
}

contract SafeComment {
    function f(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes memory b = new bytes(0x40);
        uint256[] memory xs = new uint256[](2);
        /// @solidity memory-safe-assembly
        assembly {
            let p := mload(add(xs, 0x20))
            mstore(add(p, mul(flag, 0x20)), 1)
            mstore(add(b, 0x20), mload(xs))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(b.length);
    }
}


contract Unsafe {
    function f(uint256 a, uint256 flag) external pure returns (bytes32 result) {
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
        bytes memory b = new bytes(0x40);
        uint256[] memory xs = new uint256[](2);
        assembly {
            let p := mload(add(xs, 0x20))
            mstore(add(p, mul(flag, 0x20)), 1)
            mstore(add(b, 0x20), mload(xs))
        }
        result = h0 ^ h1 ^ h2 ^ h3 ^ h4 ^ h5 ^ h6 ^ h7 ^ h8 ^ h9 ^ h10 ^ h11 ^ h12 ^ h13 ^ h14 ^ h15 ^ h16 ^ h17 ^ h18 ^ h19 ^ h20 ^ h21 ^ h22 ^ h23 ^ bytes32(b.length);
    }
}
