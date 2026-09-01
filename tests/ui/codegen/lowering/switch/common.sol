//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:

contract ProxyResult {
    // CHECK-LABEL: common.sol:ProxyResult (runtime) ===
    // CHECK: @module ProxyResult
    // CHECK-NOT: indexed_jump
    fallback() external payable {
        assembly {
            switch calldataload(0)
            case 0 { revert(0, 0) }
            default { return(0, 0) }
        }
    }
}

contract Base64Remainder {
    // CHECK-LABEL: common.sol:Base64Remainder (runtime) ===
    // CHECK: @module Base64Remainder
    // CHECK-NOT: indexed_jump
    fallback() external {
        assembly {
            switch mod(calldataload(0), 3)
            case 1 { mstore(0, 11) }
            case 2 { mstore(0, 22) }
            return(0, 32)
        }
    }
}

contract SignatureLength {
    // CHECK-LABEL: common.sol:SignatureLength (runtime) ===
    // CHECK: @module SignatureLength
    // CHECK-NOT: indexed_jump
    fallback() external {
        assembly {
            switch calldataload(0)
            case 64 { mstore(0, 27) }
            case 65 { mstore(0, 28) }
            default { mstore(0, 0) }
            return(0, 32)
        }
    }
}

contract RpowEdges {
    // CHECK-LABEL: common.sol:RpowEdges (runtime) ===
    // CHECK: @module RpowEdges
    // CHECK-NOT: indexed_jump
    fallback() external {
        assembly {
            let x := calldataload(0)
            let n := calldataload(32)
            switch x
            case 0 {
                switch n
                case 0 { mstore(0, 1) }
                default { mstore(0, 0) }
            }
            default {
                switch mod(n, 2)
                case 0 { mstore(0, 2) }
                default { mstore(0, 3) }
            }
            return(0, 32)
        }
    }
}
