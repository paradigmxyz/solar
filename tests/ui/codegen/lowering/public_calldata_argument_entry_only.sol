//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// A public function that no internal call or internal function pointer
// reaches is only entered through its selector, so its calldata arguments are
// the ones the ABI decoder checked, and hashing a range of one needs no check
// that it lies inside the calldata. A public function that is called
// internally, or whose internal function pointer is taken, can be passed a
// range that assembly set, and keeps the check.
contract PublicCalldataArgumentEntryOnly {
    // CHECK-LABEL: fn @entryOnly(
    // CHECK-NOT: calldatasize
    // CHECK: keccak256
    function entryOnly(bytes calldata data) public pure returns (bytes32) {
        return keccak256(data[1:]);
    }

    // CHECK-LABEL: fn @calledInternally(
    // CHECK: calldatasize
    // CHECK: keccak256
    function calledInternally(bytes calldata data) public pure returns (bytes32) {
        return keccak256(data[1:]);
    }

    // CHECK-LABEL: fn @pointedTo(
    // CHECK: calldatasize
    // CHECK: keccak256
    function pointedTo(bytes calldata data) public pure returns (bytes32) {
        return keccak256(data[1:]);
    }

    function caller(bytes calldata data) external pure returns (bytes32) {
        return calledInternally(data);
    }

    function viaPointer(bytes calldata data) external pure returns (bytes32) {
        function(bytes calldata) pure returns (bytes32) f = pointedTo;
        return f(data);
    }
}
