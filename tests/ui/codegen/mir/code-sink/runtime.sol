//@ codegen-matrix: standard
//@ run-call: choose false, 7, 9 => 9
//@ run-call: choose true, 7, 9 => 70
//@ run-call: choose true, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe
//@ run-call: choose false, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 2
//@ run-call: acrossStores 7 => 70, 7, 7
//@ run-call: acrossStores 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: sentry 2300, 7 => false, 2
//@ run-call: sentry 100000, 7 => true, 3
contract Sink {
    function sentry(uint256 stipend, uint256 a) external returns (bool success, uint256 stored) {
        assembly {
            sstore(1, 1)
            sstore(1, 2)
        }
        (success,) = address(this).call{gas: stipend}(abi.encodeCall(this.branchStore, (false, a)));
        assembly { stored := sload(1) }
    }

    function branchStore(bool condition, uint256 a) external returns (uint256 result) {
        assembly {
            let product := mul(a, 9)
            switch condition
            case 0 { sstore(1, 3) }
            default { result := product }
        }
    }

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
