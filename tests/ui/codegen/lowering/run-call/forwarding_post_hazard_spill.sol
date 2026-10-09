//@ codegen-matrix: standard
//@ run-call: f 0 => 0
//@ run-call: f 1 => 7237005577332262213973186563042994246635520096631353537225314386022852919809
//@ run-call: f 1024 => 7237005577332262213973186563042994246635520096631353537225314386022852919809

contract ForwardingPostHazardSpill {
    constructor() {
        assembly {
            for { let i := 0 } lt(i, 17) { i := add(i, 1) } {
                sstore(i, shl(mul(i, 8), add(i, 1)))
            }
        }
    }

    function f(uint256 length) external view returns (uint256 r) {
        assembly {
            let a0 := sload(0)
            let a1 := sload(1)
            let a2 := sload(2)
            let a3 := sload(3)
            let a4 := sload(4)
            let a5 := sload(5)
            let a6 := sload(6)
            let a7 := sload(7)
            let a8 := sload(8)
            let a9 := sload(9)
            let a10 := sload(10)
            let a11 := sload(11)
            let a12 := sload(12)
            let a13 := sload(13)
            let a14 := sload(14)
            calldatacopy(0x80, calldatasize(), length)
            if length {
                let a15 := sload(15)
                let a16 := sload(16)
                let sum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(a0, a1), a2), a3), a4), a5), a6), a7), a8), a9), a10), a11), a12), a13), a14)
                sum := add(sum, add(a15, a16))
                r := xor(sum, mul(a15, a16))
            }
        }
    }
}
