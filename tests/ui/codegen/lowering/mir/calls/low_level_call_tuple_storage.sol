//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// A tuple assignment of a low-level call stores the returned memory `bytes`
// into storage `bytes`, as a copy rather than as a storage slot.
contract LowLevelCallTupleStorage {
    bool public ok;
    bytes public result;

    // CHECK-LABEL: fn @callStored{{[( ]}}
    // CHECK: [[RET:v[0-9]+]] = icall returndata_bytes<>
    // CHECK: icall @store_storage_bytes, 1, [[RET]]
    // CHECK: sstore 0
    function callStored(address target, bytes calldata data) external {
        (ok, result) = target.call(data);
    }

    // CHECK-LABEL: fn @callSkippingSuccess{{[( ]}}
    // CHECK: [[SKIP_RET:v[0-9]+]] = icall returndata_bytes<>
    // CHECK: icall @store_storage_bytes, 1, [[SKIP_RET]]
    // CHECK-NOT: sstore 0
    function callSkippingSuccess(address target, bytes calldata data) external {
        (, result) = target.call(data);
    }

    // CHECK-LABEL: fn @staticCallStored{{[( ]}}
    // CHECK: [[STATIC_RET:v[0-9]+]] = icall returndata_bytes<>
    // CHECK: icall @store_storage_bytes, 1, [[STATIC_RET]]
    function staticCallStored(address target, bytes calldata data) external {
        (ok, result) = target.staticcall(data);
    }

    // CHECK-LABEL: fn @delegateCallStored{{[( ]}}
    // CHECK: [[DELEGATE_RET:v[0-9]+]] = icall returndata_bytes<>
    // CHECK: icall @store_storage_bytes, 1, [[DELEGATE_RET]]
    function delegateCallStored(address target, bytes calldata data) external {
        (ok, result) = target.delegatecall(data);
    }
}
