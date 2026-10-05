//@ revisions: none gas size
//@ compile-flags: --emit=bin
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//~[none]? ERROR: codegen cannot preserve values across a low-memory forwarding buffer
//~? ERROR: codegen cannot preserve values across a low-memory write in `constructor`
//~[gas,size]? ERROR: codegen cannot preserve values across a low-memory write

contract RecursiveForwarding {
    constructor(uint256 length) {
        assembly {
            let a0 := mload(0x100)
            let a1 := mload(0x120)
            let a2 := mload(0x140)
            let a3 := mload(0x160)
            let a4 := mload(0x180)
            let a5 := mload(0x1a0)
            let a6 := mload(0x1c0)
            let a7 := mload(0x1e0)
            let a8 := mload(0x200)
            let a9 := mload(0x220)
            let a10 := mload(0x240)
            let a11 := mload(0x260)
            let a12 := mload(0x280)
            let a13 := mload(0x2a0)
            let a14 := mload(0x2c0)
            calldatacopy(0x80, calldatasize(), length)
            if length {
                let a15 := mload(0x2e0)
                let a16 := mload(0x300)
                let sum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(a0, a1), a2), a3), a4), a5), a6), a7), a8), a9), a10), a11), a12), a13), a14)
                sum := add(sum, add(a15, a16))
                sstore(0, xor(xor(a15, a16), xor(sum, xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(xor(a0, a1), a2), a3), a4), a5), a6), a7), a8), a9), a10), a11), a12), a13), a14))))
            }
        }
    }

    function run(uint256 value, uint256 length, uint256 depth) external pure returns (uint256 result) {
        assembly {
            function recurse(v, n, d) -> out {
                calldatacopy(0x80, calldatasize(), n)
                if d {
                    let inner := recurse(v, n, sub(d, 1))
                    out := add(v, inner)
                    leave
                }
                out := v
            }
            result := recurse(value, length, depth)
        }
    }
}
