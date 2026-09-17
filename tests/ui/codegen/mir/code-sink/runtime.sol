//@ codegen-matrix: standard
//@ run-call: choose false, 7, 9 => 9
//@ run-call: choose true, 7, 9 => 70
//@ run-call: choose true, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call: choose false, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 2
//@ run-call: acrossStores 7 => 70, 7, 7
//@ run-call: acrossStores 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
contract Sink {
    function acrossStores(uint256 a) external returns (uint256 result, uint256 stored, uint256 memoryValue) {
        assembly {
            let product := mul(a, 9)
            sstore(0, a)
            mstore(0, a)
            result := add(product, 7)
            stored := sload(0)
            memoryValue := mload(0)
        }
    }

    function choose(bool condition, uint256 a, uint256 b) external returns (uint256 result) {
        assembly {
            let product := mul(a, 9)
            let sum := add(product, 7)
            result := b
            if condition {
                sstore(0, sum)
                result := sum
            }
        }
    }
}
