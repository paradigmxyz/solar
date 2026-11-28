//@ codegen-matrix: standard parisGas parisSize
//@ compile-flags: --allow=0067
//@[parisGas] compile-flags: --evm-version paris -O gas
//@[parisSize] compile-flags: --evm-version paris -O size
//@[mir] filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""
//@ run-call: observe [] => 0, true, 0
//@ run-call: observe [1, 255] => 0, true, 0
//@ run-call: addresses [0x0000000000000000000000000000000000000001] => 0, true, 0
pragma solidity ^0.8.0;

// CHECK: @module
contract IndexedArrayStagingMemory {
    event Narrow(uint8[] indexed a);
    event Addresses(address[] indexed a);

    // CHECK-LABEL: fn @observe{{[( ]}}
    // CHECK-NOT: keccak256_packed
    // CHECK: fmp
    // CHECK: keccak256
    // CHECK: log
    // CHECK: calldataload
    // CHECK: revert_if<empty>
    // CHECK: mstore
    function observe(uint8[] calldata a) external returns (uint256 growth, bool expanded, uint256 zero) {
        uint256 pointer; uint256 before_; uint256 after_;
        assembly ("memory-safe") { pointer := mload(0x40) before_ := msize() }
        emit Narrow(a);
        assembly ("memory-safe") { growth := sub(mload(0x40), pointer) after_ := msize() }
        uint256 expected = a.length == 0 ? before_ : pointer + a.length * 32;
        if (expected < before_) expected = before_;
        expanded = after_ == expected;
        uint256[] memory fresh = new uint256[](a.length);
        if (a.length != 0) zero = fresh[0];
    }

    // CHECK-LABEL: fn @addresses{{[( ]}}
    // CHECK-NOT: keccak256_packed
    // CHECK: fmp
    // CHECK: keccak256
    // CHECK: log
    // CHECK: calldataload
    // CHECK: revert_if<empty>
    // CHECK: mstore
    function addresses(address[] calldata a) external returns (uint256 growth, bool expanded, uint256 zero) {
        uint256 pointer; uint256 before_; uint256 after_;
        assembly ("memory-safe") { pointer := mload(0x40) before_ := msize() }
        emit Addresses(a);
        assembly ("memory-safe") { growth := sub(mload(0x40), pointer) after_ := msize() }
        uint256 expected = a.length == 0 ? before_ : pointer + a.length * 32;
        if (expected < before_) expected = before_;
        expanded = after_ == expected;
        uint256[] memory fresh = new uint256[](a.length);
        if (a.length != 0) zero = fresh[0];
    }
}
