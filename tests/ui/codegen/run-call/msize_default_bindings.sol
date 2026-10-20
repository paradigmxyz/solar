//@ codegen-matrix: standard
//@ run-call: one 0x112233, false => 0x0000000000000000000000000000000000000000000000000000000000112233, false, 256
//@ run-call: one 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001122, true => 0x0000000000000000000000000000000000000000000000000000000000001122, true, 256
//@ run-call: two 0x112233, 0xaabbccddeeff => 0x0000000000000000000000000000000000000000000000000000000000112233, 0x0000000000000000000000000000000000000000000000000000aabbccddeeff, true, 512
//@ run-call: internalCall => (7, 0)
//@ run-call: yulCall => (7, 0)
//@ run-call: pointerCall => (7, 0)
//@ run-call: operatorCall => (7, 0)
//@ run-call: writtenAtPointer => (1, 0), 4660

// Reduced from the `_init` function of zondax/solidity-BigNumber. The assembly
// takes `msize()` as its buffer and then stores that buffer into the memory
// struct `r`. The struct is allocated where it is declared, so the buffer
// starts above it and does not overwrite it.
struct BigNumber {
    bytes val;
    bool neg;
    uint256 bitlen;
}

contract BigNumberInit {
    function _init(bytes memory val, bool neg) private view returns (BigNumber memory r) {
        assembly {
            let data := add(val, 0x20)
            let length := mload(val)
            let out
            let freemem := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            switch eq(mod(length, 0x20), 0)
                case 1 {
                    out := add(freemem, 0x20)
                    mstore(freemem, length)
                }
                default {
                    let offset := sub(0x20, mod(length, 0x20))
                    out := add(add(freemem, offset), 0x20)
                    mstore(freemem, add(length, offset))
                }
            pop(staticcall(450, 0x4, data, length, out, length))
            mstore(0x40, add(freemem, add(mload(freemem), 0x20)))

            let bn_length := mload(freemem)
            for { } eq(eq(bn_length, 0x20), 0) { } {
                switch eq(mload(add(freemem, 0x20)), 0)
                    case 1 { freemem := add(freemem, 0x20) }
                    default { break }
                bn_length := sub(bn_length, 0x20)
            }
            mstore(freemem, bn_length)

            mstore(r, freemem)
            mstore(add(r, 0x20), neg)
        }
        r.bitlen = r.val.length * 8;
    }

    function one(bytes memory a, bool neg) external view returns (bytes memory, bool, uint256) {
        BigNumber memory r = _init(a, neg);
        return (r.val, r.neg, r.bitlen);
    }

    function two(bytes memory a, bytes memory b)
        external
        view
        returns (bytes memory, bytes memory, bool, uint256)
    {
        BigNumber memory x = _init(a, false);
        BigNumber memory y = _init(b, true);
        return (x.val, y.val, y.neg, x.bitlen + y.bitlen);
    }
}

struct Pair {
    uint256 a;
    uint256 b;
}

type Word is uint256;

function addWords(Word x, Word y) pure returns (Word r) {
    assembly {
        r := add(msize(), add(x, y)) //~[gas,size] WARN: `msize` is not exact when optimizing
    }
}

using {addWords as +} for Word global;

// A callee that takes memory at `msize()` must not hand out the address of the
// caller's return struct, which is allocated at the declaration.
contract CalleeMsize {
    function buffer() internal pure returns (uint256 p) {
        assembly {
            p := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
        }
    }

    function internalCall() external pure returns (Pair memory r) {
        uint256 p = buffer();
        r.a = 7;
        assembly {
            mstore(p, 99)
        }
    }

    function yulCall() external pure returns (Pair memory r) {
        uint256 p;
        assembly {
            function buf() -> q {
                q := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
            }
            p := buf()
        }
        r.a = 7;
        assembly {
            mstore(p, 99)
        }
    }

    function pointerCall() external pure returns (Pair memory r) {
        function() internal pure returns (uint256) f = buffer;
        uint256 p = f();
        r.a = 7;
        assembly {
            mstore(p, 99)
        }
    }

    function operatorCall() external pure returns (Pair memory r) {
        Word p = Word.wrap(0) + Word.wrap(0);
        r.a = 7;
        assembly {
            mstore(p, 99)
        }
    }
}

// A return variable is allocated at entry, so assembly data written at the free memory pointer
// before `msize()` is not overwritten by a later allocation.
contract WrittenAtPointer {
    function writtenAtPointer() external pure returns (Pair memory r, uint256 seen) {
        uint256 q;
        assembly {
            q := mload(0x40)
            mstore(q, 0x1234)
            seen := msize() //~[gas,size] WARN: `msize` is not exact when optimizing
        }
        assembly {
            seen := mload(q)
        }
        r.a = 1;
    }
}
