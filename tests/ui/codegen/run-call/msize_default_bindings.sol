//@ codegen-matrix: standard
//@ run-call: one 0x112233, false => 0x0000000000000000000000000000000000000000000000000000000000112233, false, 256
//@ run-call: one 0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001122, true => 0x0000000000000000000000000000000000000000000000000000000000001122, true, 256
//@ run-call: two 0x112233, 0xaabbccddeeff => 0x0000000000000000000000000000000000000000000000000000000000112233, 0x0000000000000000000000000000000000000000000000000000aabbccddeeff, true, 512

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
            let freemem := msize()
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
