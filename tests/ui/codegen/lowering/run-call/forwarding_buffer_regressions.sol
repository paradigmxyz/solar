//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: NestedLoopsHarness::run => 1
//@ run-call: RepeatedCopiesHarness::run => 1
//@ run-call: OperandShufflesHarness::run => 1
//@ run-call: DeepDestinationHarness::run => 1
//@ run-call: BuriedBaseHarness::run => 1
//@ run-call: HeapCallerHarness::run => 1
//@ run-call: RestoredPointerHarness::run => 1

// Reduced regressions for the dynamic spill base. Expected results come from a model of each
// program, not from compiling it.
// https://github.com/paradigmxyz/solar/issues/1625

function input() pure returns (bytes memory data) {
    data = new bytes(0x400);
    for (uint256 i; i < 32; i++) {
        assembly {
            mstore(add(add(data, 0x20), mul(i, 0x20)), add(mul(i, 123457), 99))
        }
    }
}

// Nested loops whose phi layout cannot also carry the base word.
contract NestedLoops {
    fallback() external {
        assembly {
            let v0 := add(calldataload(0x0), 0)
            let v1 := add(calldataload(0x20), 1)
            let v2 := add(calldataload(0x40), 2)
            let v3 := add(calldataload(0x60), 3)
            let v4 := add(calldataload(0x80), 4)
            let v5 := add(calldataload(0xa0), 5)
            let v6 := add(calldataload(0xc0), 6)
            let v7 := add(calldataload(0xe0), 7)
            let v8 := add(calldataload(0x100), 8)
            let v9 := add(calldataload(0x120), 9)
            let v10 := add(calldataload(0x140), 10)
            let v11 := add(calldataload(0x160), 11)
            let v12 := add(calldataload(0x180), 12)
            let v13 := add(calldataload(0x1a0), 13)
            calldatacopy(0, 0, calldatasize())
            for { let a := 0 } lt(a, 3) { a := add(a, 1) } { }
            for { let b := 0 } lt(b, 3) { b := add(b, 1) } {
                for { let c := 0 } lt(c, 3) { c := add(c, 1) } {
                    if lt(mod(v1, 0x100000), 0xff45e) {
                        v13 := xor(v13, keccak256(0, calldatasize()))
                        calldatacopy(0, 0, calldatasize())
                        v11 := xor(v11, keccak256(0, calldatasize()))
                    }
                }
            }
            mstore(0x0, v0)
            mstore(0x20, v1)
            mstore(0x40, v2)
            mstore(0x60, v3)
            mstore(0x80, v4)
            mstore(0xa0, v5)
            mstore(0xc0, v6)
            mstore(0xe0, v7)
            mstore(0x100, v8)
            mstore(0x120, v9)
            mstore(0x140, v10)
            mstore(0x160, v11)
            mstore(0x180, v12)
            mstore(0x1a0, v13)
            return(0, 0x1c0)
        }
    }
}

contract NestedLoopsHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new NestedLoops()).call(input());
        require(success && keccak256(result) == 0x9dbf3565eb3e4febd870085e78d0423f4239a66cb32e3e8e9537c403ba6aebe5, "NestedLoops");
        return 1;
    }
}

// Repeated copies and a conditional hash bury the base word.
contract RepeatedCopies {
    fallback() external {
        assembly {
            let v0 := add(calldataload(0x0), 0)
            let v1 := add(calldataload(0x20), 1)
            let v2 := add(calldataload(0x40), 2)
            let v3 := add(calldataload(0x60), 3)
            let v4 := add(calldataload(0x80), 4)
            let v5 := add(calldataload(0xa0), 5)
            let v6 := add(calldataload(0xc0), 6)
            let v7 := add(calldataload(0xe0), 7)
            let v8 := add(calldataload(0x100), 8)
            let v9 := add(calldataload(0x120), 9)
            let v10 := add(calldataload(0x140), 10)
            let v11 := add(calldataload(0x160), 11)
            let v12 := add(calldataload(0x180), 12)
            let v13 := add(calldataload(0x1a0), 13)
            let v14 := add(calldataload(0x1c0), 14)
            let v15 := add(calldataload(0x1e0), 15)
            let v16 := add(calldataload(0x200), 16)
            let v17 := add(calldataload(0x220), 17)
            calldatacopy(0, 0, calldatasize())
            calldatacopy(0, 0, calldatasize())
            calldatacopy(0, 0, calldatasize())
            v1 := add(v4, v17)
            if lt(mod(v13, 0x100000), 0x95bc2) { v5 := xor(v5, keccak256(0, calldatasize())) }
            mstore(0x0, v0)
            mstore(0x20, v1)
            mstore(0x40, v2)
            mstore(0x60, v3)
            mstore(0x80, v4)
            mstore(0xa0, v5)
            mstore(0xc0, v6)
            mstore(0xe0, v7)
            mstore(0x100, v8)
            mstore(0x120, v9)
            mstore(0x140, v10)
            mstore(0x160, v11)
            mstore(0x180, v12)
            mstore(0x1a0, v13)
            mstore(0x1c0, v14)
            mstore(0x1e0, v15)
            mstore(0x200, v16)
            mstore(0x220, v17)
            return(0, 0x240)
        }
    }
}

contract RepeatedCopiesHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new RepeatedCopies()).call(input());
        require(success && keccak256(result) == 0x8071bf4d30f62d1d894e05a1b7edc2246ac1f07f0f5913c32aa2f864aa0217e3, "RepeatedCopies");
        return 1;
    }
}

// Operand shuffles swap the base word below `SWAP16`.
contract OperandShuffles {
    fallback() external {
        assembly {
            let v0 := add(calldataload(0x0), 0)
            let v1 := add(calldataload(0x20), 1)
            let v2 := add(calldataload(0x40), 2)
            let v3 := add(calldataload(0x60), 3)
            let v4 := add(calldataload(0x80), 4)
            let v5 := add(calldataload(0xa0), 5)
            let v6 := add(calldataload(0xc0), 6)
            let v7 := add(calldataload(0xe0), 7)
            let v8 := add(calldataload(0x100), 8)
            let v9 := add(calldataload(0x120), 9)
            let v10 := add(calldataload(0x140), 10)
            let v11 := add(calldataload(0x160), 11)
            let v12 := add(calldataload(0x180), 12)
            let v13 := add(calldataload(0x1a0), 13)
            let v14 := add(calldataload(0x1c0), 14)
            let v15 := add(calldataload(0x1e0), 15)
            let v16 := add(calldataload(0x200), 16)
            let v17 := add(calldataload(0x220), 17)
            let v18 := add(calldataload(0x240), 18)
            let v19 := add(calldataload(0x260), 19)
            let v20 := add(calldataload(0x280), 20)
            let v21 := add(calldataload(0x2a0), 21)
            let v22 := add(calldataload(0x2c0), 22)
            let v23 := add(calldataload(0x2e0), 23)
            calldatacopy(0, 0, calldatasize())
            v7 := add(v4, v2)
            v2 := xor(v21, v14)
            mstore(add(0x20, mul(0x20, mod(v8, 16))), v12)
            v13 := mul(v10, v2)
            v3 := sub(v1, v8)
            mstore(0x0, v0)
            mstore(0x20, v1)
            mstore(0x40, v2)
            mstore(0x60, v3)
            mstore(0x80, v4)
            mstore(0xa0, v5)
            mstore(0xc0, v6)
            mstore(0xe0, v7)
            mstore(0x100, v8)
            mstore(0x120, v9)
            mstore(0x140, v10)
            mstore(0x160, v11)
            mstore(0x180, v12)
            mstore(0x1a0, v13)
            mstore(0x1c0, v14)
            mstore(0x1e0, v15)
            mstore(0x200, v16)
            mstore(0x220, v17)
            mstore(0x240, v18)
            mstore(0x260, v19)
            mstore(0x280, v20)
            mstore(0x2a0, v21)
            mstore(0x2c0, v22)
            mstore(0x2e0, v23)
            return(0, 0x300)
        }
    }
}

contract OperandShufflesHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new OperandShuffles()).call(input());
        require(success && keccak256(result) == 0x0093688666209845e56a66351f35dde250d17e00f50d3f261a93e343108f153d, "OperandShuffles");
        return 1;
    }
}

// A word store's destination needs a deep spill.
contract DeepDestination {
    fallback() external {
        assembly {
            let v0 := add(calldataload(0x0), 0)
            let v1 := add(calldataload(0x20), 1)
            let v2 := add(calldataload(0x40), 2)
            let v3 := add(calldataload(0x60), 3)
            let v4 := add(calldataload(0x80), 4)
            let v5 := add(calldataload(0xa0), 5)
            let v6 := add(calldataload(0xc0), 6)
            let v7 := add(calldataload(0xe0), 7)
            let v8 := add(calldataload(0x100), 8)
            let v9 := add(calldataload(0x120), 9)
            let v10 := add(calldataload(0x140), 10)
            let v11 := add(calldataload(0x160), 11)
            let v12 := add(calldataload(0x180), 12)
            let v13 := add(calldataload(0x1a0), 13)
            let v14 := add(calldataload(0x1c0), 14)
            let v15 := add(calldataload(0x1e0), 15)
            let v16 := add(calldataload(0x200), 16)
            let v17 := add(calldataload(0x220), 17)
            calldatacopy(0, 0, calldatasize())
            mstore(add(0x20, mul(0x20, mod(v0, 16))), v8)
            v4 := add(v3, v13)
            v0 := mul(v17, v17)
            v14 := add(v8, mload(0))
            mstore(0x0, v0)
            mstore(0x20, v1)
            mstore(0x40, v2)
            mstore(0x60, v3)
            mstore(0x80, v4)
            mstore(0xa0, v5)
            mstore(0xc0, v6)
            mstore(0xe0, v7)
            mstore(0x100, v8)
            mstore(0x120, v9)
            mstore(0x140, v10)
            mstore(0x160, v11)
            mstore(0x180, v12)
            mstore(0x1a0, v13)
            mstore(0x1c0, v14)
            mstore(0x1e0, v15)
            mstore(0x200, v16)
            mstore(0x220, v17)
            return(0, 0x240)
        }
    }
}

contract DeepDestinationHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new DeepDestination()).call(input());
        require(success && keccak256(result) == 0x91715611a5d2cb2e5cd859aebefda22f89786ab5e992b8d4a8a66badae52d7f2, "DeepDestination");
        return 1;
    }
}

// A spill access finds the base word below `DUP16` and parks the words above it.
contract BuriedBase {
    fallback() external {
        assembly {
            let v0 := add(calldataload(0x0), 0)
            let v1 := add(calldataload(0x20), 1)
            let v2 := add(calldataload(0x40), 2)
            let v3 := add(calldataload(0x60), 3)
            let v4 := add(calldataload(0x80), 4)
            let v5 := add(calldataload(0xa0), 5)
            let v6 := add(calldataload(0xc0), 6)
            let v7 := add(calldataload(0xe0), 7)
            let v8 := add(calldataload(0x100), 8)
            let v9 := add(calldataload(0x120), 9)
            let v10 := add(calldataload(0x140), 10)
            let v11 := add(calldataload(0x160), 11)
            let v12 := add(calldataload(0x180), 12)
            let v13 := add(calldataload(0x1a0), 13)
            let v14 := add(calldataload(0x1c0), 14)
            let v15 := add(calldataload(0x1e0), 15)
            let v16 := add(calldataload(0x200), 16)
            let v17 := add(calldataload(0x220), 17)
            let v18 := add(calldataload(0x240), 18)
            let v19 := add(calldataload(0x260), 19)
            let v20 := add(calldataload(0x280), 20)
            let v21 := add(calldataload(0x2a0), 21)
            let v22 := add(calldataload(0x2c0), 22)
            let v23 := add(calldataload(0x2e0), 23)
            let v24 := add(calldataload(0x300), 24)
            let v25 := add(calldataload(0x320), 25)
            let v26 := add(calldataload(0x340), 26)
            let v27 := add(calldataload(0x360), 27)
            let v28 := add(calldataload(0x380), 28)
            let v29 := add(calldataload(0x3a0), 29)
            calldatacopy(0, 0, calldatasize())
            v28 := sub(v25, v27)
            v10 := v5
            v22 := mul(v9, v6)
            mstore(add(0x20, mul(0x20, mod(v16, 16))), v23)
            mstore(0x0, v0)
            mstore(0x20, v1)
            mstore(0x40, v2)
            mstore(0x60, v3)
            mstore(0x80, v4)
            mstore(0xa0, v5)
            mstore(0xc0, v6)
            mstore(0xe0, v7)
            mstore(0x100, v8)
            mstore(0x120, v9)
            mstore(0x140, v10)
            mstore(0x160, v11)
            mstore(0x180, v12)
            mstore(0x1a0, v13)
            mstore(0x1c0, v14)
            mstore(0x1e0, v15)
            mstore(0x200, v16)
            mstore(0x220, v17)
            mstore(0x240, v18)
            mstore(0x260, v19)
            mstore(0x280, v20)
            mstore(0x2a0, v21)
            mstore(0x2c0, v22)
            mstore(0x2e0, v23)
            mstore(0x300, v24)
            mstore(0x320, v25)
            mstore(0x340, v26)
            mstore(0x360, v27)
            mstore(0x380, v28)
            mstore(0x3a0, v29)
            return(0, 0x3c0)
        }
    }
}

contract BuriedBaseHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new BuriedBase()).call(input());
        require(success && keccak256(result) == 0xbfe5de367651c97787d7b3afa98dd7b6af76525fa80d78b5b83394cd61f6c246, "BuriedBase");
        return 1;
    }
}

// A callee allocates from the free-memory pointer after the spill area moved.
contract HeapCaller {
    function g(uint256 n) internal pure returns (uint256 s) {
        uint256[] memory a = new uint256[](n);
        for (uint256 i; i < n; i++) a[i] = i;
        s = a[n - 1];
    }

    function f(uint256 x) external pure returns (uint256, uint256) {
        x = x * 3 + 1;
        assembly {
            calldatacopy(0x60, 0, calldatasize())
        }
        uint256 r = g(80);
        return (x, r);
    }
}

contract HeapCallerHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) =
            address(new HeapCaller()).call(abi.encodePacked(abi.encodeCall(HeapCaller.f, (7)), new bytes(512)));
        require(success, "heap caller call");
        (uint256 x, uint256 r) = abi.decode(result, (uint256, uint256));
        require(x == 22 && r == 79, "heap caller");
        return 1;
    }
}

// Assembly restores the free-memory pointer after using low memory, then a callee allocates.
contract RestoredPointer {
    function enc(uint256 a, uint256 b) internal pure returns (bytes memory) {
        return abi.encode(a, b, a ^ b);
    }

    function f(uint256 x) external pure returns (bytes32 h, bytes32 e, uint256 y) {
        y = x * 5 + 3;
        assembly {
            let m := mload(0x40)
            calldatacopy(0, 0, calldatasize())
            h := keccak256(0, calldatasize())
            mstore(0x40, m)
            mstore(0x60, 0)
        }
        e = keccak256(enc(y, x));
    }
}

contract RestoredPointerHarness {
    function run() external returns (uint256) {
        bytes memory data = abi.encodePacked(abi.encodeCall(RestoredPointer.f, (7)), new bytes(512));
        (bool success, bytes memory result) = address(new RestoredPointer()).call(data);
        require(success, "restored pointer call");
        (bytes32 h, bytes32 e, uint256 y) = abi.decode(result, (bytes32, bytes32, uint256));
        require(h == keccak256(data) && e == keccak256(abi.encode(38, 7, 33)) && y == 38, "restored pointer");
        return 1;
    }
}
