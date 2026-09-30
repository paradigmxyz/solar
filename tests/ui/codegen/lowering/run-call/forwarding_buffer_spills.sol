//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: NineHarness::run => 1
//@ run-call: PostCopyHarness::run => 1
//@ run-call: BranchHarness::run => 1
//@ run-call: SweepHarness::run => 1
//@ run-call: AppendHarness::run => 1
//@ run-call: InternalCallHarness::run => 1
//@ run-call: ProxyHarness::run => 1
//@ run-call: ConstructorCopyHarness::run => 1
//@ run-call: ConstantWriteHarness::run => 1
//@ run-call: LiveAboveHarness::run => 1
//@ run-call: InternalCopyHarness::run => 1
//@ run-call: LoopCarriedHarness::run => 1
//@ run-call: StackArgsHarness::run => 1

// A dynamic-length write from low memory, such as `calldatacopy(0, 0, calldatasize())`,
// can cover every fixed spill slot, and the written buffer stays readable afterwards.
// Values live across such a write, and values spilled after it, must survive without
// touching the buffer, however many there are.
// https://github.com/paradigmxyz/solar/issues/1625
// The standard matrix's `mir` revision would snapshot the MIR of every contract here
// without testing anything the runtime calls do not.

// The reported shape: nine values across the copy and a join after the conditional log.
contract Nine {
    fallback() external {
        assembly {
            calldatacopy(0x100, 0x20, 0x120)
            let a := mload(0x100)
            let b := mload(0x120)
            let c := mload(0x140)
            let d := mload(0x160)
            let e := mload(0x180)
            let f := mload(0x1a0)
            let g := mload(0x1c0)
            let h := mload(0x1e0)
            let i := mload(0x200)

            calldatacopy(0, 0, calldatasize())
            if calldataload(0) { log1(0, 0, a) }

            mstore(0, add(add(add(add(a, b), add(c, d)), add(add(e, f), add(g, h))), i))
            return(0, 0x20)
        }
    }
}

// Values created after the copy spill while the buffer is still read.
contract PostCopy {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0, calldatasize())
            calldatacopy(0, 0, calldatasize())
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            let v18 := mload(0x10240)
            let v19 := mload(0x10260)
            codecopy(0x10000, 0, 0x280)
            mstore(0x20000, add(v0, v19))
            let digest := keccak256(0, calldatasize())

            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0x240, v17)
            mstore(0x260, v18)
            mstore(0x280, v19)
            mstore(0, digest)
            return(0, 0x2a0)
        }
    }
}

// Only one path copies; the join reloads spills written on either path.
contract Branch {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0x20, 0x280)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            let v18 := mload(0x10240)
            let v19 := mload(0x10260)
            if calldataload(0) { calldatacopy(0, 0, calldatasize()) }
            let digest := keccak256(0, calldatasize())

            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0x240, v17)
            mstore(0x260, v18)
            mstore(0x280, v19)
            mstore(0, digest)
            return(0, 0x2a0)
        }
    }
}

// A decompressor-style loop sweeps word stores upward from address zero.
contract Sweep {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0x20, 0x240)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            for { let p := 0 } lt(p, calldatasize()) { p := add(p, 0x20) } {
                mstore(p, calldataload(p))
            }
            let digest := keccak256(0, calldatasize())

            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0x240, v17)
            mstore(0, digest)
            return(0, 0x260)
        }
    }
}

// A forwarder appends the sender right after the copied calldata.
contract Append {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0x20, 0x220)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            calldatacopy(0, 0, calldatasize())
            mstore(calldatasize(), shl(96, caller()))
            let digest := keccak256(0, add(calldatasize(), 20))

            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0, digest)
            return(0, 0x240)
        }
    }
}

// Recursive internal calls run while the values are live. The copy starts above the
// free-memory pointer, which dynamic frames need, and the buffer is hashed before the
// calls because their frames may land in it.
contract InternalCall {
    function fib(uint256 n) internal returns (uint256) {
        if (n < 2) return n;
        return fib(n - 1) + fib(n - 2);
    }

    fallback() external {
        uint256 v0;
        uint256 v1;
        uint256 v2;
        uint256 v3;
        uint256 v4;
        uint256 v5;
        uint256 v6;
        uint256 v7;
        uint256 v8;
        uint256 v9;
        uint256 v10;
        uint256 v11;
        uint256 v12;
        uint256 v13;
        uint256 v14;
        uint256 v15;
        uint256 v16;
        uint256 v17;
        assembly {
            calldatacopy(0x10000, 0x20, 0x240)
            v0 := mload(0x10000)
            v1 := mload(0x10020)
            v2 := mload(0x10040)
            v3 := mload(0x10060)
            v4 := mload(0x10080)
            v5 := mload(0x100a0)
            v6 := mload(0x100c0)
            v7 := mload(0x100e0)
            v8 := mload(0x10100)
            v9 := mload(0x10120)
            v10 := mload(0x10140)
            v11 := mload(0x10160)
            v12 := mload(0x10180)
            v13 := mload(0x101a0)
            v14 := mload(0x101c0)
            v15 := mload(0x101e0)
            v16 := mload(0x10200)
            v17 := mload(0x10220)
            calldatacopy(0x60, 0, calldatasize())
        }
        bytes32 digest;
        assembly {
            digest := keccak256(0x60, calldatasize())
        }
        uint256 r = fib(msg.data.length % 11);
        assembly {
            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0x240, v17)
            mstore(0, digest)
            mstore(0x260, r)
            return(0, 0x280)
        }
    }
}

contract Implementation {
    fallback() external {
        assembly {
            calldatacopy(0, 0, calldatasize())
            mstore(0, keccak256(0, calldatasize()))
            mstore(0x20, calldatasize())
            return(0, 0x40)
        }
    }
}

// A proxy forwards the copy, copies the return data back, and appends values after it.
contract Proxy {
    address internal immutable implementation;

    constructor(address target) {
        implementation = target;
    }

    fallback() external {
        address target = implementation;
        assembly {
            calldatacopy(0x10000, 0, 0x240)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            calldatacopy(0, 0, calldatasize())
            let ok := delegatecall(gas(), target, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            if iszero(ok) { revert(0, returndatasize()) }
            let size := returndatasize()
            mstore(add(size, 0x0), v0)
            mstore(add(size, 0x20), v1)
            mstore(add(size, 0x40), v2)
            mstore(add(size, 0x60), v3)
            mstore(add(size, 0x80), v4)
            mstore(add(size, 0xa0), v5)
            mstore(add(size, 0xc0), v6)
            mstore(add(size, 0xe0), v7)
            mstore(add(size, 0x100), v8)
            mstore(add(size, 0x120), v9)
            mstore(add(size, 0x140), v10)
            mstore(add(size, 0x160), v11)
            mstore(add(size, 0x180), v12)
            mstore(add(size, 0x1a0), v13)
            mstore(add(size, 0x1c0), v14)
            mstore(add(size, 0x1e0), v15)
            mstore(add(size, 0x200), v16)
            mstore(add(size, 0x220), v17)
            return(0, add(size, 0x240))
        }
    }
}

// A constant-address write lands right above the memory used before the move.
contract ConstantWrite {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0x20, 0x220)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            calldatacopy(0, 0, calldatasize())
            mstore(0x10240, not(0))
            if calldataload(0) { log4(add(v0, 1), add(v1, 2), mul(v2, v3), xor(v4, v5), sub(v6, v16), v15) }

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
            return(0, 0x220)
        }
    }
}

// Data stored above the buffer before the copy is still read after it.
contract LiveAbove {
    fallback() external {
        assembly {
            let seed := add(calldatasize(), 0x200)
            calldatacopy(seed, 0x20, 0x220)
            let v0 := mload(add(seed, 0x0))
            let v1 := mload(add(seed, 0x20))
            let v2 := mload(add(seed, 0x40))
            let v3 := mload(add(seed, 0x60))
            let v4 := mload(add(seed, 0x80))
            let v5 := mload(add(seed, 0xa0))
            let v6 := mload(add(seed, 0xc0))
            let v7 := mload(add(seed, 0xe0))
            let v8 := mload(add(seed, 0x100))
            let v9 := mload(add(seed, 0x120))
            let v10 := mload(add(seed, 0x140))
            let v11 := mload(add(seed, 0x160))
            let v12 := mload(add(seed, 0x180))
            let v13 := mload(add(seed, 0x1a0))
            let v14 := mload(add(seed, 0x1c0))
            let v15 := mload(add(seed, 0x1e0))
            let v16 := mload(add(seed, 0x200))
            mstore(add(calldatasize(), 0x20), 0x1234)
            calldatacopy(0, 0, calldatasize())
            let marker := mload(add(calldatasize(), 0x20))
            if calldataload(0) { log4(add(v0, 1), add(v1, 2), mul(v2, v3), xor(v4, v5), sub(v6, v16), v15) }

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
            mstore(0x220, marker)
            return(0, 0x240)
        }
    }
}

// An internal function owns the copy and the live values.
contract InternalCopy {
    function work(uint256 salt) internal returns (bytes32 digest, uint256 total) {
        assembly {
            calldatacopy(0x10000, 0x20, 0x240)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            calldatacopy(0, 0, calldatasize())
            if eq(salt, 7) { log4(add(v0, 1), add(v1, 2), mul(v2, v3), xor(v4, v5), sub(v6, v17), v16) }
            digest := keccak256(0, calldatasize())
            total := add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(v0, 3), v1), 3), v2), 3), v3), 3), v4), 3), v5), 3), v6), 3), v7), 3), v8), 3), v9), 3), v10), 3), v11), 3), v12), 3), v13), 3), v14), 3), v15), 3), v16), 3), v17)
        }
        total += salt;
    }

    fallback() external {
        (bytes32 digest, uint256 total) = work(msg.data.length % 5);
        assembly {
            mstore(0, digest)
            mstore(0x20, total)
            return(0, 0x40)
        }
    }
}

// Loop-carried values cross a copy on every iteration.
contract LoopCarried {
    fallback() external {
        assembly {
            calldatacopy(0x10000, 0x20, 0x220)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            for { let i := 0 } lt(i, 3) { i := add(i, 1) } {
                calldatacopy(0, 0, calldatasize())
                v0 := add(v0, add(i, mload(0x20)))
                v1 := add(v1, add(i, mload(0x40)))
                v2 := add(v2, add(i, mload(0x60)))
                v3 := add(v3, add(i, mload(0x80)))
                v4 := add(v4, add(i, mload(0xa0)))
                v5 := add(v5, add(i, mload(0xc0)))
                v6 := add(v6, add(i, mload(0xe0)))
                v7 := add(v7, add(i, mload(0x100)))
                v8 := add(v8, add(i, mload(0x120)))
                v9 := add(v9, add(i, mload(0x140)))
                v10 := add(v10, add(i, mload(0x160)))
                v11 := add(v11, add(i, mload(0x180)))
                v12 := add(v12, add(i, mload(0x1a0)))
                v13 := add(v13, add(i, mload(0x1c0)))
                v14 := add(v14, add(i, mload(0x1e0)))
                v15 := add(v15, add(i, mload(0x200)))
                v16 := add(v16, add(i, mload(0x220)))
            }
            let digest := keccak256(0, calldatasize())

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
            mstore(0x220, digest)
            return(0, 0x240)
        }
    }
}

// A non-recursive helper with several arguments runs after the copy.
contract StackArgs {
    function mix(uint256 a, uint256 b, uint256 c, uint256 d) internal pure returns (uint256 r) {
        for (uint256 i; i < 4; i++) {
            r = r * 31 + (a ^ b) + (c * d) + i;
            (a, b, c, d) = (b, c, d, a + 1);
        }
    }

    fallback() external {
        uint256 v0;
        uint256 v1;
        uint256 v2;
        uint256 v3;
        uint256 v4;
        uint256 v5;
        uint256 v6;
        uint256 v7;
        uint256 v8;
        uint256 v9;
        uint256 v10;
        uint256 v11;
        uint256 v12;
        uint256 v13;
        uint256 v14;
        uint256 v15;
        uint256 v16;
        uint256 v17;
        bytes32 digest;
        assembly {
            calldatacopy(0x10000, 0x20, 0x240)
            v0 := mload(0x10000)
            v1 := mload(0x10020)
            v2 := mload(0x10040)
            v3 := mload(0x10060)
            v4 := mload(0x10080)
            v5 := mload(0x100a0)
            v6 := mload(0x100c0)
            v7 := mload(0x100e0)
            v8 := mload(0x10100)
            v9 := mload(0x10120)
            v10 := mload(0x10140)
            v11 := mload(0x10160)
            v12 := mload(0x10180)
            v13 := mload(0x101a0)
            v14 := mload(0x101c0)
            v15 := mload(0x101e0)
            v16 := mload(0x10200)
            v17 := mload(0x10220)
            calldatacopy(0, 0, calldatasize())
            digest := keccak256(0, calldatasize())
        }
        uint256 r = mix(v0, v1, v2, v3) ^ mix(v17, v16, v15, msg.data.length);
        assembly {
            mstore(0x20, v0)
            mstore(0x40, v1)
            mstore(0x60, v2)
            mstore(0x80, v3)
            mstore(0xa0, v4)
            mstore(0xc0, v5)
            mstore(0xe0, v6)
            mstore(0x100, v7)
            mstore(0x120, v8)
            mstore(0x140, v9)
            mstore(0x160, v10)
            mstore(0x180, v11)
            mstore(0x1a0, v12)
            mstore(0x1c0, v13)
            mstore(0x1e0, v14)
            mstore(0x200, v15)
            mstore(0x220, v16)
            mstore(0x240, v17)
            mstore(0, digest)
            mstore(0x260, r)
            return(0, 0x280)
        }
    }
}

// Constructor code copies its own code over low memory.
contract ConstructorCopy {
    uint256 public result;
    bytes32 public digest;

    constructor(bytes memory payload) {
        uint256 r;
        bytes32 d;
        assembly {
            mcopy(0x10000, add(payload, 0x20), 0x280)
            let v0 := mload(0x10000)
            let v1 := mload(0x10020)
            let v2 := mload(0x10040)
            let v3 := mload(0x10060)
            let v4 := mload(0x10080)
            let v5 := mload(0x100a0)
            let v6 := mload(0x100c0)
            let v7 := mload(0x100e0)
            let v8 := mload(0x10100)
            let v9 := mload(0x10120)
            let v10 := mload(0x10140)
            let v11 := mload(0x10160)
            let v12 := mload(0x10180)
            let v13 := mload(0x101a0)
            let v14 := mload(0x101c0)
            let v15 := mload(0x101e0)
            let v16 := mload(0x10200)
            let v17 := mload(0x10220)
            let v18 := mload(0x10240)
            let v19 := mload(0x10260)
            codecopy(0, 0, codesize())
            if iszero(codesize()) { log1(0, 0, v0) }
            d := keccak256(0, codesize())
            r := add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(add(mul(v0, 3), v1), 3), v2), 3), v3), 3), v4), 3), v5), 3), v6), 3), v7), 3), v8), 3), v9), 3), v10), 3), v11), 3), v12), 3), v13), 3), v14), 3), v15), 3), v16), 3), v17), 3), v18), 3), v19)
        }
        result = r;
        digest = d;
    }
}

// Calldata longer than any static spill area forces the area to move.
function padding(uint256 count) pure returns (bytes memory) {
    return words(count, 77, 0xabc);
}

function words(uint256 count, uint256 step, uint256 offset) pure returns (bytes memory data) {
    data = new bytes(count * 32);
    for (uint256 i; i < count; i++) {
        assembly {
            mstore(add(add(data, 0x20), mul(i, 0x20)), add(mul(i, step), offset))
        }
    }
}

function prefix(bytes memory data, uint256 length) pure returns (bytes memory result) {
    result = new bytes(length);
    assembly {
        mcopy(add(result, 0x20), add(data, 0x20), length)
    }
}

contract NineHarness {
    function run() external returns (uint256) {
        address target = address(new Nine());
        bytes memory values = abi.encode(11, 12, 13, 14, 15, 16, 17, 18, 19);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                (bool success, bytes memory result) =
                    target.call(abi.encodePacked(flag, values, padding(pad * 128)));
                require(success && abi.decode(result, (uint256)) == 135, "nine");
            }
        }
        return 1;
    }
}

contract PostCopyHarness {
    function run() external returns (uint256) {
        address target = address(new PostCopy());
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(words(32, 1111, 5), padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "post-copy call");
            require(
                keccak256(result) == keccak256(abi.encodePacked(keccak256(data), prefix(data, 0x280))),
                "post-copy"
            );
        }
        return 1;
    }
}

contract BranchHarness {
    function run() external returns (uint256) {
        address target = address(new Branch());
        bytes memory values = words(20, 1000, 9);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                bytes memory data = abi.encodePacked(flag, values, padding(pad * 128));
                (bool success, bytes memory result) = target.call(data);
                require(success && result.length == 0x2a0, "branch call");
                bytes32 digest;
                assembly {
                    digest := mload(add(result, 0x20))
                }
                bytes memory returned = new bytes(0x280);
                assembly {
                    mcopy(add(returned, 0x20), add(result, 0x40), 0x280)
                }
                require(keccak256(returned) == keccak256(values), "branch values");
                require(flag == 0 || digest == keccak256(data), "branch digest");
            }
        }
        return 1;
    }
}

contract SweepHarness {
    function run() external returns (uint256) {
        address target = address(new Sweep());
        for (uint256 pad; pad < 2; pad++) {
            bytes memory values = words(24, 1000, 3);
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "sweep call");
            require(
                keccak256(result) == keccak256(abi.encodePacked(keccak256(data), prefix(values, 0x240))),
                "sweep"
            );
        }
        return 1;
    }
}

contract AppendHarness {
    function run() external returns (uint256) {
        address target = address(new Append());
        bytes memory values = words(17, 1000, 1);
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "append call");
            bytes32 digest = keccak256(abi.encodePacked(data, address(this)));
            require(keccak256(result) == keccak256(abi.encodePacked(digest, values)), "append");
        }
        return 1;
    }
}

contract InternalCallHarness {
    function run() external returns (uint256) {
        address target = address(new InternalCall());
        bytes memory values = words(18, 1000, 5);
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "internal call");
            (uint256 fib, uint256 next) = (0, 1);
            for (uint256 i; i < data.length % 11; i++) {
                (fib, next) = (next, fib + next);
            }
            require(
                keccak256(result) == keccak256(abi.encodePacked(keccak256(data), values, fib)),
                "internal"
            );
        }
        return 1;
    }
}

contract ProxyHarness {
    function run() external returns (uint256) {
        address target = address(new Proxy(address(new Implementation())));
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(words(32, 1000, 11), padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "proxy call");
            require(
                keccak256(result)
                    == keccak256(abi.encodePacked(keccak256(data), data.length, prefix(data, 0x240))),
                "proxy"
            );
        }
        return 1;
    }
}

contract ConstructorCopyHarness {
    function run() external returns (uint256) {
        ConstructorCopy target = new ConstructorCopy(words(20, 1000, 7));
        uint256 expected = 7;
        for (uint256 i = 1; i < 20; i++) {
            expected = expected * 3 + (i * 1000 + 7);
        }
        require(target.result() == expected && target.digest() != bytes32(0), "constructor");
        return 1;
    }
}

contract ConstantWriteHarness {
    function run() external returns (uint256) {
        address target = address(new ConstantWrite());
        bytes memory values = words(17, 1000, 13);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                (bool success, bytes memory result) =
                    target.call(abi.encodePacked(flag, values, padding(pad * 128)));
                require(success && keccak256(result) == keccak256(values), "constant write");
            }
        }
        return 1;
    }
}

contract LiveAboveHarness {
    function run() external returns (uint256) {
        address target = address(new LiveAbove());
        bytes memory values = words(17, 1000, 17);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                (bool success, bytes memory result) =
                    target.call(abi.encodePacked(flag, values, padding(pad * 128)));
                require(
                    success && keccak256(result) == keccak256(abi.encodePacked(values, uint256(0x1234))),
                    "live above"
                );
            }
        }
        return 1;
    }
}

contract InternalCopyHarness {
    function run() external returns (uint256) {
        address target = address(new InternalCopy());
        bytes memory values = words(18, 1000, 21);
        uint256 expected = 21;
        for (uint256 i = 1; i < 18; i++) {
            expected = expected * 3 + (i * 1000 + 21);
        }
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "internal copy call");
            require(
                keccak256(result) == keccak256(abi.encode(keccak256(data), expected + data.length % 5)),
                "internal copy"
            );
        }
        return 1;
    }
}

contract LoopCarriedHarness {
    function run() external returns (uint256) {
        address target = address(new LoopCarried());
        bytes memory values = words(17, 1000, 5);
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "loop call");
            bytes memory expected = new bytes(0x220);
            for (uint256 k; k < 17; k++) {
                uint256 word = k * 1000 + 5;
                assembly {
                    mstore(add(add(expected, 0x20), mul(k, 0x20)), add(mul(word, 4), 3))
                }
            }
            require(
                keccak256(result) == keccak256(abi.encodePacked(expected, keccak256(data))),
                "loop"
            );
        }
        return 1;
    }
}

contract StackArgsHarness {
    function mix(uint256 a, uint256 b, uint256 c, uint256 d) internal pure returns (uint256 r) {
        for (uint256 i; i < 4; i++) {
            r = r * 31 + (a ^ b) + (c * d) + i;
            (a, b, c, d) = (b, c, d, a + 1);
        }
    }

    function run() external returns (uint256) {
        address target = address(new StackArgs());
        bytes memory values = words(18, 1000, 3);
        for (uint256 pad; pad < 2; pad++) {
            bytes memory data = abi.encodePacked(uint256(0), values, padding(pad * 128));
            (bool success, bytes memory result) = target.call(data);
            require(success, "stack args call");
            uint256 r = mix(3, 1003, 2003, 3003) ^ mix(17003, 16003, 15003, data.length);
            require(
                keccak256(result) == keccak256(abi.encodePacked(keccak256(data), values, r)),
                "stack args"
            );
        }
        return 1;
    }
}
