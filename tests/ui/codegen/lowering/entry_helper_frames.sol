//@ codegen-matrix: standard layout
//@[layout] compile-flags: -Ogas -Zdump=mir-final
//@[layout] filecheck: --check-prefix=LAYOUT
//@ run-call: wide 3 => 0x2b818cac54ba249dc82afa56e2ad2ce55f8e5ef935bf5782acceaed2ab70b7d1
//@ run-call: narrow 3 => 104
//@ run-call: pinned 3 => 420
//@ run-call: addressed 0x100, 3 => 247

// Each helper keeps twenty values live across its loop, so it spills into a static frame.
// `wide` names low memory by constants, which raises the dispatcher's own area and every
// region placed above it. A helper starts above the spill area and static allocation of the
// entry that calls it, not above the dispatcher's: `narrow`'s helper starts at the heap start
// and `pinned`'s above its array. `addressed` writes memory at a calldata-computed address,
// which raises its own base and the helper above it.
// LAYOUT: // backend-frames 2 0x460
// LAYOUT: // backend-alloc 3 0 static 0x80
// LAYOUT: // backend-frames 3 0x4c0
// LAYOUT: // backend-frames 4 0x2460
contract EntryHelperFrames {
    function wide(uint256 x) external pure returns (uint256 r) {
        assembly {
            mstore(0x80, x)
            mstore(0x500, x)
            r := keccak256(0x80, 0x500)
        }
    }

    function narrow(uint256 x) external pure returns (uint256) {
        return heavyA(x);
    }

    function pinned(uint256 x) external pure returns (uint256) {
        uint256[3] memory a = [x, x + 1, x + 2];
        uint256 r = heavyC(x);
        return r + a[0] + a[1] + a[2];
    }

    function addressed(uint256 p, uint256 x) external pure returns (uint256 r) {
        assembly {
            mstore(p, x)
        }
        r = heavyB(x);
        assembly {
            r := add(r, mload(p))
        }
    }

    function heavyA(uint256 x) internal pure returns (uint256) {
        uint256 v0 = x + 1;
        uint256 v1 = x + 2;
        uint256 v2 = x + 3;
        uint256 v3 = x + 4;
        uint256 v4 = x + 5;
        uint256 v5 = x + 6;
        uint256 v6 = x + 7;
        uint256 v7 = x + 8;
        uint256 v8 = x + 9;
        uint256 v9 = x + 10;
        uint256 v10 = x + 11;
        uint256 v11 = x + 12;
        uint256 v12 = x + 13;
        uint256 v13 = x + 14;
        uint256 v14 = x + 15;
        uint256 v15 = x + 16;
        uint256 v16 = x + 17;
        uint256 v17 = x + 18;
        uint256 v18 = x + 19;
        uint256 v19 = x + 20;
        for (uint256 i; i < x; ++i) { v0 += v1; v1 += v2; v2 += v3; v3 += v4; v4 += v5; v5 += v6; v6 += v7; v7 += v8; v8 += v9; v9 += v10; v10 += v11; v11 += v12; v12 += v13; v13 += v14; v14 += v15; v15 += v16; v16 += v17; v17 += v18; v18 += v19; v19 += v0; }
        return v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7 ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14 ^ v15 ^ v16 ^ v17 ^ v18 ^ v19;
    }

    function heavyB(uint256 x) internal pure returns (uint256) {
        uint256 v0 = x + 7;
        uint256 v1 = x + 8;
        uint256 v2 = x + 9;
        uint256 v3 = x + 10;
        uint256 v4 = x + 11;
        uint256 v5 = x + 12;
        uint256 v6 = x + 13;
        uint256 v7 = x + 14;
        uint256 v8 = x + 15;
        uint256 v9 = x + 16;
        uint256 v10 = x + 17;
        uint256 v11 = x + 18;
        uint256 v12 = x + 19;
        uint256 v13 = x + 20;
        uint256 v14 = x + 21;
        uint256 v15 = x + 22;
        uint256 v16 = x + 23;
        uint256 v17 = x + 24;
        uint256 v18 = x + 25;
        uint256 v19 = x + 26;
        for (uint256 i; i < x; ++i) { v0 += v1; v1 += v2; v2 += v3; v3 += v4; v4 += v5; v5 += v6; v6 += v7; v7 += v8; v8 += v9; v9 += v10; v10 += v11; v11 += v12; v12 += v13; v13 += v14; v14 += v15; v15 += v16; v16 += v17; v17 += v18; v18 += v19; v19 += v0; }
        return v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7 ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14 ^ v15 ^ v16 ^ v17 ^ v18 ^ v19;
    }

    function heavyC(uint256 x) internal pure returns (uint256) {
        uint256 v0 = x + 13;
        uint256 v1 = x + 14;
        uint256 v2 = x + 15;
        uint256 v3 = x + 16;
        uint256 v4 = x + 17;
        uint256 v5 = x + 18;
        uint256 v6 = x + 19;
        uint256 v7 = x + 20;
        uint256 v8 = x + 21;
        uint256 v9 = x + 22;
        uint256 v10 = x + 23;
        uint256 v11 = x + 24;
        uint256 v12 = x + 25;
        uint256 v13 = x + 26;
        uint256 v14 = x + 27;
        uint256 v15 = x + 28;
        uint256 v16 = x + 29;
        uint256 v17 = x + 30;
        uint256 v18 = x + 31;
        uint256 v19 = x + 32;
        for (uint256 i; i < x; ++i) { v0 += v1; v1 += v2; v2 += v3; v3 += v4; v4 += v5; v5 += v6; v6 += v7; v7 += v8; v8 += v9; v9 += v10; v10 += v11; v11 += v12; v12 += v13; v13 += v14; v14 += v15; v15 += v16; v16 += v17; v17 += v18; v18 += v19; v19 += v0; }
        return v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7 ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14 ^ v15 ^ v16 ^ v17 ^ v18 ^ v19;
    }
}
