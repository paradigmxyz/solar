//@compile-flags: -O none -Zdump=mir
//@filecheck:

contract YulLocalPhi {
    // CHECK-LABEL: fn @branchLocal{{[( ]}}
    // CHECK: jumpi {{v[0-9]+}},
    // CHECK: {{v[0-9]+}} = phi [bb1: 2], [bb2: 1]
    // CHECK: ret {{v[0-9]+}}
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
