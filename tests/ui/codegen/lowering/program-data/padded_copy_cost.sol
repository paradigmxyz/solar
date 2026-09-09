//@ codegen-matrix: standard
//@ run-call: shortTail => 63, 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e00
//@ run-call: aligned => 64, 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f
//@ run-call: longTail => 65, 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0x4000000000000000000000000000000000000000000000000000000000000000
//@ run-call: repeated => 95, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa00

// Check logical lengths and initialized padding independently of the chosen copy strategy.
contract PaddedCopyCost {
    function shortTail() external pure returns (uint256 length, bytes32 first, bytes32 last) {
        dirtyNextAllocation();
        bytes memory value = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e";
        assembly {
            length := mload(value)
            first := mload(add(value, 32))
            last := mload(add(value, and(add(length, 31), not(31))))
        }
    }

    function aligned() external pure returns (uint256 length, bytes32 first, bytes32 last) {
        dirtyNextAllocation();
        bytes memory value = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f";
        assembly {
            length := mload(value)
            first := mload(add(value, 32))
            last := mload(add(value, and(add(length, 31), not(31))))
        }
    }

    function longTail() external pure returns (uint256 length, bytes32 first, bytes32 last) {
        dirtyNextAllocation();
        bytes memory value = hex"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40";
        assembly {
            length := mload(value)
            first := mload(add(value, 32))
            last := mload(add(value, and(add(length, 31), not(31))))
        }
    }

    function repeated() external pure returns (uint256 length, bytes32 first, bytes32 last) {
        dirtyNextAllocation();
        bytes memory value = hex"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assembly {
            length := mload(value)
            first := mload(add(value, 32))
            last := mload(add(value, and(add(length, 31), not(31))))
        }
    }

    function dirtyNextAllocation() private pure {
        assembly {
            let free := mload(64)
            mstore(add(free, 32), not(0))
            mstore(add(free, 64), not(0))
            mstore(add(free, 96), not(0))
        }
    }
}
