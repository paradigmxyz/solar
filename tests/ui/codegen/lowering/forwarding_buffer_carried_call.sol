//@ compile-flags: -O gas --emit=bin

// Too many values live across a call to a helper that writes memory it never allocated,
// after a low-memory copy. No spill address is safe from the helper, storage loads cannot be
// recomputed after it, and the values do not fit on the stack, so codegen reports it instead
// of emitting code that loses them.
// https://github.com/paradigmxyz/solar/issues/1625

contract C {
    function append(uint256 n) internal view {
        for (uint256 i = 0; i < n; i++) {
            assembly { mstore(add(0x80, add(calldatasize(), mul(i, 0x20))), caller()) }
        }
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(sload(0x0), 0) }
        uint256 v1;
        assembly { v1 := add(sload(0x20), 1) }
        uint256 v2;
        assembly { v2 := add(sload(0x40), 2) }
        uint256 v3;
        assembly { v3 := add(sload(0x60), 3) }
        uint256 v4;
        assembly { v4 := add(sload(0x80), 4) }
        uint256 v5;
        assembly { v5 := add(sload(0xa0), 5) }
        uint256 v6;
        assembly { v6 := add(sload(0xc0), 6) }
        uint256 v7;
        assembly { v7 := add(sload(0xe0), 7) }
        uint256 v8;
        assembly { v8 := add(sload(0x100), 8) }
        uint256 v9;
        assembly { v9 := add(sload(0x120), 9) }
        uint256 v10;
        assembly { v10 := add(sload(0x140), 10) }
        uint256 v11;
        assembly { v11 := add(sload(0x160), 11) }
        uint256 v12;
        assembly { v12 := add(sload(0x180), 12) }
        uint256 v13;
        assembly { v13 := add(sload(0x1a0), 13) }
        uint256 v14;
        assembly { v14 := add(sload(0x1c0), 14) }
        uint256 v15;
        assembly { v15 := add(sload(0x1e0), 15) }
        uint256 v16;
        assembly { v16 := add(sload(0x200), 16) }
        uint256 v17;
        assembly { v17 := add(sload(0x220), 17) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        append(2);
        append(1);
        uint256 r = v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7 ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14 ^ v15 ^ v16 ^ v17;
        assembly {
            mstore(0, r)
            return(0, 0x20)
        }
    }
}

//~? ERROR: codegen cannot keep 18 values of `fallback` on the stack across an internal call after a dynamic low-memory write
