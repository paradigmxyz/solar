//@ codegen-matrix: standard
//@ run-call: overlap => 1
//@ run-call: overlapValue 0 => 1
//@ run-call: overlapValue 1 => 1
//@ run-call: overlapValue 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 1

contract LatePartialOverlap {
    function overlap() external returns (uint256) {
        assembly {
            mstore(128, 1)
            log0(128, 32)
            mstore(129, 0)
            mstore(128, 1)
            return(128, 32)
        }
    }
    function overlapValue(uint256 value) external returns (uint256) {
        assembly {
            mstore(128, 1)
            log0(128, 32)
            mstore(129, value)
            mstore(128, 1)
            return(128, 32)
        }
    }
}
