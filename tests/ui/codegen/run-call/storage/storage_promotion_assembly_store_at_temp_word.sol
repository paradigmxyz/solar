//@ codegen-matrix: standard
//@ run-call: tail 3 => 8
//@ run-call: body 3 => 8
//@ run-call: pair 3 => 16
//@ run-call: copy 3 => 8

// Assembly stores to fixed addresses between a promoted slot's initial store and its flush must
// not reach the memory temporary that holds the slot.
contract StoragePromotionAssemblyStoreAtTempWord {
    uint256 public x;
    uint256 public y;

    function tail(uint256 n) external returns (uint256) {
        x = 5;
        assembly { mstore(0x80, 0x1111) }
        for (uint256 i; i < n; ++i) x += 1;
        return x;
    }

    function body(uint256 n) external returns (uint256) {
        x = 5;
        for (uint256 i; i < n; ++i) {
            x += 1;
            assembly { mstore(0x80, 0x1111) }
        }
        return x;
    }

    function copy(uint256 n) external returns (uint256) {
        x = 5;
        assembly { calldatacopy(0x80, 0, calldatasize()) }
        for (uint256 i; i < n; ++i) x += 1;
        return x;
    }

    function pair(uint256 n) external returns (uint256) {
        x = 5;
        y = 7;
        assembly {
            mstore(0xa0, 0x2222)
            mstore(0xc0, 0x3333)
        }
        for (uint256 i; i < n; ++i) {
            x += 1;
            y += 1;
        }
        return x + y - 2;
    }
}
