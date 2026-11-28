//@ compile-flags: -O none -Zdump=mir
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

contract EventIndexedCalldataStaging {
    event Words(uint256[] indexed values);
    event Narrow(uint8[] indexed values);
    event Addresses(address[] indexed values);
    event Signed(int256[] indexed values);

    // CHECK-LABEL: fn @words{{[( ]}}
    // CHECK-NOT: alloc
    // CHECK: keccak256_packed (array<word> calldata arg0)
    // CHECK: log
    function words(uint256[] calldata values) external {
        emit Words(values);
    }

    // Validated unsigned calldata arrays fuse validation and FMP staging into one loop.
    // CHECK-LABEL: fn @narrow{{[( ]}}
    // CHECK-NOT: alloc
    // CHECK-NOT: keccak256_packed
    // CHECK: fmp
    // CHECK: keccak256
    // CHECK: log
    // CHECK: calldataload
    // CHECK: revert_if<empty>
    // CHECK: mstore
    function narrow(uint8[] calldata values) external {
        emit Narrow(values);
    }

    // CHECK-LABEL: fn @addresses{{[( ]}}
    // CHECK-NOT: alloc
    // CHECK-NOT: keccak256_packed
    // CHECK: fmp
    // CHECK: keccak256
    // CHECK: log
    // CHECK: calldataload
    // CHECK: revert_if<empty>
    // CHECK: mstore
    function addresses(address[] calldata values) external {
        emit Addresses(values);
    }

    // Full-width wordwise calldata keeps its block-copy path through memory.
    // CHECK-LABEL: fn @signed{{[( ]}}
    // CHECK: alloc
    // CHECK: keccak256_packed (array<word, wordwise> memoryarray<1>
    function signed(int256[] calldata values) external {
        emit Signed(values);
    }
}
