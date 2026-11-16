//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract CalldataArraySubslice {
    // A sub-slice of a word-element calldata array materializes correctly: the
    // slice value carries the data pointer and length, so a word copy from the
    // adjusted position rebuilds the memory array.
    // CHECK-LABEL: fn @word{{[( ]}}
    // CHECK: [[START:v[0-9]+]] = ptradd {{v[0-9]+}}, 32
    // CHECK-NEXT: [[SUB:v[0-9]+]] = make_calldata_slice [[START]], {{v[0-9]+}}
    // CHECK: [[SUB_PTR:v[0-9]+]] = extract_value slice<calldata>, [[SUB]], 0
    // CHECK: [[OUT:v[0-9]+]] = alloc
    // CHECK-NEXT: mstore [[OUT]], {{v[0-9]+}}
    // CHECK-NEXT: [[SRC:v[0-9]+]] = make_calldata_slice [[SUB_PTR]], {{v[0-9]+}}
    // CHECK-NEXT: [[DATA:v[0-9]+]] = ptradd [[OUT]], 32
    // CHECK-NEXT: [[SRC_PTR:v[0-9]+]] = extract_value slice<calldata>, [[SRC]], 0
    // CHECK-NEXT: [[SRC_LEN:v[0-9]+]] = extract_value slice<calldata>, [[SRC]], 1
    // CHECK-NEXT: calldatacopy [[DATA]], [[SRC_PTR]], [[SRC_LEN]]
    function word(uint256[] calldata a) external pure returns (uint256[] memory) {
        return a[1:];
    }
}
