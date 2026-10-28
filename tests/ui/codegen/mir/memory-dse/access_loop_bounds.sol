//@ codegen-matrix: standard
//@ run-call: previousIteration => 7

contract AccessLoopBounds {
    function previousIteration() external pure returns (uint256 result) {
        assembly {
            let free := mload(0x40)
            let carried := 0x80
            mstore(0, 7)
            mstore(0x80, 0)
            mstore(0x40, not(31))
            for { let i := 0 } lt(i, 2) { i := add(i, 1) } {
                let current := mload(0x40)
                switch i
                case 0 {
                    carried := add(current, 32)
                    mstore(0x40, 0x80)
                }
                default {
                    result := add(mload(current), mload(carried))
                }
            }
            mstore(0x40, free)
        }
    }
}
