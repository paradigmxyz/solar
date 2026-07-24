//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract YulLocalPhi {
    // CHECK-LABEL: fn @branchLocal{{[( ]}}
    // CHECK: mstore 160, 1
    // CHECK: jumpi {{v[0-9]+}},
    // CHECK: mstore 160, 2
    // CHECK: {{v[0-9]+}} = mload 160
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
