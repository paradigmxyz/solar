//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Osize -Zdump=mir
//@[dump] filecheck:
//@ run-call: memoryCopy 0, 0, 3 => 0x112233
//@ run-call: memoryCopy 0x10000000000000000, 0, 0 => 0x000000
//@ run-call: memoryCopy 0, 0x10000000000000000, 0 => 0x000000
//@ run-call: memoryCopy 0x10000000000000000, 0x10000000000000000, 0 => 0x000000
//@ run-call-fail: memoryCopy 0x10000000000000000, 0, 1
//@ run-call-fail: memoryCopy 0, 0x10000000000000000, 1
//@ run-call: calldataCopy 0x112233, 0, 2 => 0x112200
//@ run-call: calldataCopy 0x, 0x10000000000000000, 0 => 0x000000
//@ run-call-fail: calldataCopy 0x11, 0x10000000000000000, 1
//@ run-call: downCopy 0, 0, 2 => 0x112200
//@ run-call: downCopy 0x10000000000000000, 0x10000000000000000, 0 => 0x000000
//@ run-call: freshCopy 0x112233, 0, 2 => 0x1122
//@ run-call: freshCopy 0x, 0x10000000000000000, 0 => 0x

// Zero iterations must not load either buffer's length, even under -Osize. The forged
// pointers would exhaust gas on any access; masking only the panic condition is not enough.
contract Test {
    function memoryCopy(uint256 sourceAt, uint256 destinationAt, uint256 count)
        external pure returns (bytes memory result)
    {
        bytes memory source = hex"112233";
        result = new bytes(3);
        bytes memory destination = result;
        assembly {
            if sourceAt { source := sourceAt }
            if destinationAt { destination := destinationAt }
        }
        _memoryCopy(source, destination, count);
    }

    function calldataCopy(bytes calldata source, uint256 destinationAt, uint256 count)
        external pure returns (bytes memory result)
    {
        result = new bytes(3);
        bytes memory destination = result;
        assembly {
            if destinationAt { destination := destinationAt }
        }
        _calldataCopy(source, destination, count);
    }

    function downCopy(uint256 sourceAt, uint256 destinationAt, uint256 count)
        external pure returns (bytes memory result)
    {
        bytes memory source = hex"112233";
        result = new bytes(3);
        bytes memory destination = result;
        assembly {
            if sourceAt { source := sourceAt }
            if destinationAt { destination := destinationAt }
        }
        _downCopy(source, destination, count);
    }

    function freshCopy(bytes memory source, uint256 sourceAt, uint256 count)
        external pure returns (bytes memory result)
    {
        assembly {
            if sourceAt { source := sourceAt }
        }
        return _freshCopy(source, count);
    }

    // CHECK-LABEL: fn @_memoryCopy(
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg2, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: mload
    // CHECK: mcopy
    /// @custom:solar-inline never
    function _memoryCopy(bytes memory source, bytes memory destination, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            destination[k] = source[k];
        }
    }

    // CHECK-LABEL: fn @_calldataCopy(
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg3, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: mload
    // CHECK: calldatacopy
    /// @custom:solar-inline never
    function _calldataCopy(bytes calldata source, bytes memory destination, uint256 count) private pure {
        for (uint256 k; k < count; ++k) {
            destination[k] = source[k];
        }
    }

    // CHECK-LABEL: fn @_downCopy(
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg2, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: mload
    // CHECK: mcopy
    /// @custom:solar-inline never
    function _downCopy(bytes memory source, bytes memory destination, uint256 count) private pure {
        for (uint256 k = count; k > 0; --k) {
            destination[k - 1] = source[k - 1];
        }
    }

    // CHECK-LABEL: fn @_freshCopy(
    // CHECK: [[EMPTY:v[0-9]+]] = eq arg1, 0
    // CHECK-NEXT: jumpi [[EMPTY]]
    // CHECK: mload
    // CHECK: mcopy
    /// @custom:solar-inline never
    function _freshCopy(bytes memory source, uint256 count) private pure returns (bytes memory result) {
        result = new bytes(count);
        for (uint256 k; k < count; ++k) {
            result[k] = source[k];
        }
        assembly ("memory-safe") {
            mstore(add(add(result, 32), count), 0)
        }
    }
}
