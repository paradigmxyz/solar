//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract YulExtCalls {
    // CHECK-LABEL: fn @extCalls{{[( ]}}
    // CHECK: {{v[0-9]+}} = extcall arg0, 0, 0, 0
    // CHECK: {{v[0-9]+}} = extdelegatecall arg0, 0, 0
    // CHECK: {{v[0-9]+}} = extstaticcall arg0, 0, 0
    function extCalls(address target) public returns (uint256 result) {
        assembly {
            pop(extcall(target, 0, 0, 0))
            pop(extdelegatecall(target, 0, 0))
            result := extstaticcall(target, 0, 0)
        }
    }
}
