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
//@ run-call: AppendingCalleeHarness::run => 1
//@ run-call: AllocatingCalleeHarness::run => 1
//@ run-call: RecursiveCarryHarness::run => 1
//@ run-call: ConstructorLoopHarness::run => 1
//@ run-call: CarriedStackArgsHarness::run => 1
//@ run-call: StackReturnCallHarness::run => 1
//@ run-call: CopyingCalleeLoopHarness::run => 1
//@ run-call: IfElseReturnHarness::run => 1
//@ run-call: SwitchExitHarness::run => 1
//@ run-call: CopyingHelperHarness::run => 1

// Reduced regressions for the dynamic spill base. Expected results come from a model of each
// program, not from compiling it.
// https://github.com/paradigmxyz/solar/issues/1625
// The standard matrix's `mir` revision would snapshot the MIR of every contract here
// without testing anything the runtime calls do not.

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

// A callee's assembly appends right after the copied buffer.
contract AppendingCallee {
    function append(uint256 n) internal view {
        for (uint256 i = 0; i < n; i++) {
            assembly { mstore(add(0x80, add(calldatasize(), mul(i, 0x20))), caller()) }
        }
    }

    fallback() external {
        uint256 v0;
        assembly { v0 := add(calldataload(0), 7) }
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        append(2);
        append(1);
        uint256 r = v0;
        assembly { mstore(0, r) return(0, 0x20) }
    }
}

contract AppendingCalleeHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new AppendingCallee()).call(input());
        require(success && abi.decode(result, (uint256)) == 106, "appending callee");
        return 1;
    }
}

// The caller allocates before a callee allocates, after moving the free-memory pointer itself.
contract AllocatingCallee {
    function g() internal pure returns (bytes32) {
        return keccak256(new bytes(200));
    }

    fallback() external {
        assembly {
            calldatacopy(0x80, 0, calldatasize())
            mstore(0x40, add(0x180, calldatasize()))
        }
        uint256 v0;
        assembly { v0 := add(mload(0x80), 0) }
        uint256 v1;
        assembly { v1 := add(mload(0xa0), 1) }
        uint256 v2;
        assembly { v2 := add(mload(0xc0), 2) }
        uint256 v3;
        assembly { v3 := add(mload(0xe0), 3) }
        bytes memory m = new bytes(1024);
        bytes32 h = g();
        uint256 r = v0 ^ v1 ^ v2 ^ v3;
        assembly { mstore(0, r) mstore(0x20, mload(m)) mstore(0x40, h) return(0, 0x60) }
    }
}

contract AllocatingCalleeHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new AllocatingCallee()).call(input());
        require(success, "allocating callee call");
        (uint256 r, uint256 length, bytes32 h) = abi.decode(result, (uint256, uint256, bytes32));
        require(r == 491784 && length == 1024 && h == keccak256(new bytes(200)), "allocating callee");
        return 1;
    }
}

// Each recursive activation copies over low memory, and a child's frame comes from the
// free-memory pointer.
contract RecursiveCarry {
    function rec(uint256 n, uint256 x) internal returns (uint256 r) {
        unchecked {
            uint256 v0 = n * 3 + 0 + x;
            uint256 v1 = n * 4 + 1 + x;
            uint256 v2 = n * 5 + 2 + x;
            uint256 v3 = n * 6 + 3 + x;
            assembly { calldatacopy(0x100, 0, calldatasize()) }
            if (n > 0) x = rec(n - 1, x + 1);
            r = (v0 ^ v1 ^ v2 ^ v3) + x;
        }
    }

    function run(uint256 n) external returns (uint256) {
        return rec(n, 0);
    }
}

contract RecursiveCarryHarness {
    function run() external returns (uint256) {
        RecursiveCarry target = new RecursiveCarry();
        for (uint256 n = 1; n < 4; n += 2) {
            (bool success, bytes memory result) =
                address(target).call(abi.encodePacked(abi.encodeCall(RecursiveCarry.run, (n)), new bytes(512)));
            require(success && abi.decode(result, (uint256)) == (n == 1 ? 0xd : 0x17), "recursive carry");
        }
        return 1;
    }
}

// A constructor copies its code over low memory inside a loop.
contract ConstructorLoop {
    uint256 public out;

    constructor(uint256 x) {
        unchecked {
            uint256 v0 = x * 3 + 0;
            uint256 v1 = x * 4 + 1;
            uint256 v2 = x * 5 + 2;
            uint256 v3 = x * 6 + 3;
            uint256 v4 = x * 7 + 4;
            uint256 v5 = x * 8 + 5;
            uint256 v6 = x * 9 + 6;
            uint256 v7 = x * 10 + 7;
            uint256 v8 = x * 11 + 8;
            uint256 v9 = x * 12 + 9;
            uint256 v10 = x * 13 + 10;
            uint256 v11 = x * 14 + 11;
            uint256 v12 = x * 15 + 12;
            uint256 v13 = x * 16 + 13;
            uint256 v14 = x * 17 + 14;
            uint256 v15 = x * 18 + 15;
            uint256 v16 = x * 19 + 16;
            uint256 v17 = x * 20 + 17;
            uint256 v18 = x * 21 + 18;
            uint256 v19 = x * 22 + 19;
            assembly { codecopy(0, 0, codesize()) }
            for (uint256 i = 0; i < 3; i++) {
                v19 += v18 * i;
                assembly { codecopy(0x40, 0, codesize()) }
            }
            out = (v0 * 1) ^ (v1 * 2) ^ (v2 * 3) ^ (v3 * 4) ^ (v4 * 5) ^ (v5 * 6) ^ (v6 * 7) ^ (v7 * 8) ^ (v8 * 9) ^ (v9 * 10) ^ (v10 * 11) ^ (v11 * 12) ^ (v12 * 13) ^ (v13 * 14) ^ (v14 * 15) ^ (v15 * 16) ^ (v16 * 17) ^ (v17 * 18) ^ (v18 * 19) ^ (v19 * 20);
        }
    }
}

contract ConstructorLoopHarness {
    function run() external returns (uint256) {
        require(new ConstructorLoop(5).out() == 9068, "constructor loop");
        return 1;
    }
}

// Values carried across a helper's call are also its stack-passed arguments.
contract CarriedStackArgs {
    function f0(uint256 a0, uint256 a1, uint256 a2, uint256 a3) internal pure returns (uint256) {
        unchecked {
            uint256 l0 = a2 * 3 + 0;
            uint256 l1 = a3 * 4 + 1;
            uint256 l2 = a3 * 5 + 2;
            return l0 ^ l1 ^ l2;
        }
    }
    function f1(uint256 a0, uint256 a1, uint256 a2, uint256 a3) internal pure returns (uint256) {
        unchecked {
            uint256 l0 = a1 * 3 + 0;
            uint256 l1 = a1 * 4 + 1;
            uint256 l2 = a2 * 5 + 2;
            uint256 l3 = a1 * 6 + 3;
            uint256 l4 = a0 * 7 + 4;
            uint256 l5 = a2 * 8 + 5;
            uint256 l6 = a1 * 9 + 6;
            uint256 l7 = a2 * 10 + 7;
            uint256 l8 = a0 * 11 + 8;
            uint256 l9 = a0 * 12 + 9;
            uint256 l10 = a2 * 13 + 10;
            uint256 l11 = a3 * 14 + 11;
            uint256 l12 = a0 * 15 + 12;
            assembly { calldatacopy(0x60, 0, calldatasize()) }
            return l0 ^ l1 ^ l2 ^ l3 ^ l4 ^ l5 ^ l6 ^ l7 ^ l8 ^ l9 ^ l10 ^ l11 ^ l12;
        }
    }
    function run(uint256 x) external pure returns (uint256) {
        uint256 y = f1(x + 0, x + 1, x + 2, x + 3);
        assembly { calldatacopy(0, 0, calldatasize()) }
        return y ^ f1(x + 0, x + 1, x + 2, x + 3);
    }
}

contract CarriedStackArgsHarness {
    function run() external returns (uint256) {
        (bool success, bytes memory result) = address(new CarriedStackArgs()).call(
            abi.encodePacked(abi.encodeCall(CarriedStackArgs.run, (7)), new bytes(512))
        );
        require(success && abi.decode(result, (uint256)) == 0, "carried stack args");
        return 1;
    }
}

// A carried call in a nested loop to a callee that returns on the stack at `-O size`; the
// inner loop exit, emitted before the call, still reloads a slot the call moved.
contract StackReturnCall {
    function h(uint256 a0, uint256 a1) internal pure returns (uint256 r) {
        unchecked {
            r = a0;
            for (uint256 k = 0; k < (a1 & 3) + 1; k++) {
                r = r * 39 + a1;
                r ^= a0 >> 3;
            }
            if (a0 & 7 == 0) {
                uint256[] memory m = new uint256[]((a0 & 15) + 1);
                m[0] = r;
                r += m[0] + m.length;
            }
        }
    }
    function f(uint256 seed) external pure returns (uint256 acc) {
        unchecked {
            uint256 v0 = seed;
            uint256 v1 = seed * 3 + 1;
            uint256 v2 = seed * 5 + 4;
            uint256 v3 = seed * 7 + 9;
            uint256 v4 = seed * 9 + 16;
            uint256 v5 = seed * 11 + 25;
            assembly { calldatacopy(0, 0, calldatasize()) }
            if (v2 % 3 == 1) {
                v3 = v0 * (893 | v4) + (v2 & v5) * (464 ^ 615);
            }
            for (uint256 i = 0; i < v3 % 3; i++) {
                for (uint256 j = 0; j < v0 % 3; j++) {
                    v4 = v5 ^ v2;
                    v2 = h(v0 ^ 950, 802) + v1;
                }
            }
            acc = v0 ^ (v1 << 1) ^ (v2 << 2) ^ (v3 << 3) ^ (v4 << 4) ^ (v5 << 5);
        }
    }
}

contract StackReturnCallHarness {
    function run() external returns (uint256) {
        address target = address(new StackReturnCall());
        uint256[4] memory seeds = [uint256(0x1234567), 3, 1000, 77];
        uint256[4] memory expected =
            [uint256(0x42073d1947f), 0x26f73, 0x1a7cfaa, 0x3470ac75];
        for (uint256 i; i < 4; i++) {
            (bool success, bytes memory result) = target.call(
                abi.encodePacked(abi.encodeCall(StackReturnCall.f, (seeds[i])), new bytes(512))
            );
            require(success && abi.decode(result, (uint256)) == expected[i], "stack return call");
        }
        return 1;
    }
}

// A loop that calls a helper which copies into low memory itself.
contract CopyingCalleeLoop {
    function g(uint256 a) internal pure returns (uint256 r) {
        unchecked {
            assembly { calldatacopy(0, 0, calldatasize()) }
            r = a * 3 + 1;
        }
    }
    function f(uint256 seed) external pure returns (uint256 acc) {
        unchecked {
            uint256 v0 = seed;
            uint256 v1 = seed * 3 + 1;
            uint256 v2 = seed * 5 + 4;
            uint256 v3 = seed * 7 + 9;
            uint256 v4 = seed * 9 + 16;
            uint256 v5 = seed * 11 + 25;
            assembly { calldatacopy(0, 0, calldatasize()) }
            for (uint256 i = 0; i < v3 % 4; i++) {
                v4 = v5 ^ v2;
                v2 = g(v0 ^ i) + v1;
            }
            acc = v0 ^ (v1 << 1) ^ (v2 << 2) ^ (v3 << 3) ^ (v4 << 4) ^ (v5 << 5);
        }
    }
}

contract CopyingCalleeLoopHarness {
    function run() external returns (uint256) {
        address target = address(new CopyingCalleeLoop());
        uint256[4] memory seeds = [uint256(0x1234567), 3, 1000, 77];
        uint256[4] memory expected = [uint256(0x11673841f), 0x503, 0x679aa, 0x57f9];
        for (uint256 i; i < 4; i++) {
            (bool success, bytes memory result) = target.call(
                abi.encodePacked(abi.encodeCall(CopyingCalleeLoop.f, (seeds[i])), new bytes(512))
            );
            require(success && abi.decode(result, (uint256)) == expected[i], "copying callee");
        }
        return 1;
    }
}

// Both arms return, so `-O none` keeps an unreachable join block.
contract IfElseReturn {
    function f(uint256 s) external pure returns (uint256) {
        unchecked {
            uint256 a0 = s;
            uint256 a1 = s * 3 + 1;
            uint256 a2 = s * 5 + 2;
            uint256 a3 = s * 7 + 3;
            uint256 a4 = s * 9 + 4;
            uint256 a5 = s * 11 + 5;
            uint256 a6 = s * 13 + 6;
            uint256 a7 = s * 15 + 7;
            uint256 a8 = s * 17 + 8;
            uint256 a9 = s * 19 + 9;
            uint256 b0 = s * 21 + 10;
            uint256 b1 = s * 23 + 11;
            uint256 b2 = s * 25 + 12;
            uint256 b3 = s * 27 + 13;
            uint256 b4 = s * 29 + 14;
            uint256 b5 = s * 31 + 15;
            uint256 b6 = s * 33 + 16;
            uint256 b7 = s * 35 + 17;
            uint256 b8 = s * 37 + 18;
            uint256 b9 = s * 39 + 19;
            assembly { calldatacopy(0, 0, calldatasize()) }
            uint256 a = a0 ^ a1 ^ a2 ^ a3 ^ a4 ^ a5 ^ a6 ^ a7 ^ a8 ^ a9;
            uint256 b = b0 ^ b1 ^ b2 ^ b3 ^ b4 ^ b5 ^ b6 ^ b7 ^ b8 ^ b9;
            if (s & 1 == 0) {
                return a;
            } else {
                return b;
            }
        }
    }
}

contract IfElseReturnHarness {
    function run() external returns (uint256) {
        address target = address(new IfElseReturn());
        uint256[4] memory seeds = [uint256(0x1234567), 3, 1000, 77];
        uint256[4] memory expected = [uint256(0x8a2bab1), 0x19, 0x931, 0xd2d];
        for (uint256 i; i < 4; i++) {
            (bool success, bytes memory result) = target.call(
                abi.encodePacked(abi.encodeCall(IfElseReturn.f, (seeds[i])), new bytes(512))
            );
            require(success && abi.decode(result, (uint256)) == expected[i], "if else return");
        }
        return 1;
    }
}

// OpenZeppelin's `Proxy._delegate` shape, forwarding to the identity precompile.
contract SwitchExit {
    fallback() external {
        assembly {
            calldatacopy(0, 0, calldatasize())
            let ok := staticcall(gas(), 4, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch ok
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }
}

contract SwitchExitHarness {
    function run() external returns (uint256) {
        bytes memory data = input();
        (bool success, bytes memory result) = address(new SwitchExit()).call(data);
        require(success && keccak256(result) == keccak256(data), "switch exit");
        return 1;
    }
}

// A caller with many live values calls a helper that copies over low memory.
contract CopyingHelper {
    function z(uint256 a0, uint256 a1) internal pure returns (uint256 r) {
        unchecked {
            uint256 l0 = a0 * 3 + 1;
            uint256 l1 = a1 * 5 + 2;
            uint256 l2 = a0 * 7 + 3;
            uint256 l3 = a1 * 9 + 4;
            uint256 l4 = a0 * 11 + 5;
            uint256 l5 = a1 * 13 + 6;
            uint256 l6 = a0 * 15 + 7;
            uint256 l7 = a1 * 17 + 8;
            uint256 l8 = a0 * 19 + 9;
            assembly { calldatacopy(0, 0, calldatasize()) }
            for (uint256 i = 0; i < (a0 & 3); i++) {
                l0 = l0 * 31 + l8;
                l8 ^= l1;
            }
            r = l0 ^ l1 ^ l2 ^ l3 ^ l4 ^ l5 ^ l6 ^ l7 ^ l8;
        }
    }
    function f(uint256 s) external pure returns (uint256) {
        unchecked {
            uint256 v0 = s;
            uint256 v1 = s * 3 + 1;
            uint256 v2 = s * 5 + 2;
            uint256 v3 = s * 7 + 3;
            uint256 v4 = s * 9 + 4;
            uint256 v5 = s * 11 + 5;
            uint256 v6 = s * 13 + 6;
            uint256 v7 = s * 15 + 7;
            uint256 v8 = s * 17 + 8;
            uint256 v9 = s * 19 + 9;
            uint256 v10 = s * 21 + 10;
            uint256 v11 = s * 23 + 11;
            uint256 v12 = s * 25 + 12;
            uint256 v13 = s * 27 + 13;
            uint256 v14 = s * 29 + 14;
            uint256 v15 = s * 31 + 15;
            uint256 v16 = z(v2, v4);
            uint256 v17 = s * 35 + 17;
            uint256 v18 = s * 37 + 18;
            return v0 ^ v1 ^ v2 ^ v3 ^ v4 ^ v5 ^ v6 ^ v7 ^ v8 ^ v9 ^ v10 ^ v11 ^ v12 ^ v13 ^ v14
                ^ v15 ^ v16 ^ v17 ^ v18;
        }
    }
}

contract CopyingHelperHarness {
    function run() external returns (uint256) {
        address target = address(new CopyingHelper());
        uint256[4] memory seeds = [uint256(0x1234567), 3, 1000, 77];
        uint256[4] memory expected = [uint256(0x234d1b502), 0x546, 0x108fc5a, 0x27e6dc1];
        for (uint256 i; i < 4; i++) {
            (bool success, bytes memory result) = target.call(
                abi.encodePacked(abi.encodeCall(CopyingHelper.f, (seeds[i])), new bytes(512))
            );
            require(success && abi.decode(result, (uint256)) == expected[i], "copying helper");
        }
        return 1;
    }
}
