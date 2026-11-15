//@revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck:

contract Caller {
    // CHECK-LABEL: fn @probe
    // CHECK: address_staticcall
    function probe(address target) external view returns (bool) {
        (bool success,) = target.staticcall("");
        //~[homestead]^ ERROR: builtin `staticcall` requires Byzantium-compatible EVM
        return success;
    }
}
