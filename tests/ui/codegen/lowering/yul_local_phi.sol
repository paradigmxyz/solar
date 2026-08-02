//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract YulLocalPhi {
    // CHECK-LABEL: fn @branchLocal{{[( ]}}
    // CHECK: frame_store scratch, word, 32, 1
    // CHECK: jumpi {{v[0-9]+}},
    // CHECK: frame_store scratch, word, 32, 2
    // CHECK: {{v[0-9]+}} = frame_load scratch, word, 32
    function branchLocal(uint256 flag) public pure returns (uint256 result) {
        assembly {
            let x := 1
            if flag {
                x := 2
            }
            result := x
        }
    }
}
