//@ codegen-matrix: standard
//@ run-call: run 4096, false => 79
//@ run-call: run 4096, true => 86

contract StackPhiResidentSource {
    constructor() {
        assembly {
            for { let i := 0 } lt(i, 12) { i := add(i, 1) } {
                sstore(i, add(i, 1))
            }
        }
    }

    function run(uint256 length, bool choose) external returns (uint256) {
        assembly {
            let x0 := sload(0)
            let x1 := sload(1)
            let x2 := sload(2)
            let x3 := sload(3)
            let x4 := sload(4)
            let x5 := sload(5)
            let x6 := sload(6)
            let x7 := sload(7)
            let x8 := sload(8)
            let x9 := sload(9)
            let x10 := sload(10)
            let x11 := sload(11)
            calldatacopy(0x80, calldatasize(), length)
            let r := x0
            if choose {
                log0(0, 0)
                r := add(x0, 7)
            }
            mstore(0, add(add(add(add(add(add(add(add(add(add(add(add(r, x0), x1), x2), x3), x4), x5), x6), x7), x8), x9), x10), x11))
            return(0, 32)
        }
    }
}
