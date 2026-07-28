//@revisions: homestead byzantium
//@[homestead] compile-flags: -Zcodegen -O none --evm-version homestead -Zdump=mir
//@[byzantium] compile-flags: -Zcodegen -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck:

contract Caller {
    // CHECK-LABEL: fn @probe
    // CHECK: staticcall
    function probe(address target) external view returns (bool) {
        (bool success,) = target.staticcall("");
        //~[homestead]^ ERROR: codegen cannot use `staticcall` before Byzantium
        return success;
    }

    // CHECK-LABEL: fn @probeCall
    // CHECK: call
    // CHECK: returndatasize
    // CHECK: returndatacopy
    function probeCall(address target) external returns (uint256) {
        (, bytes memory data) = target.call("");
        //~[homestead]^ ERROR: codegen cannot bind low-level call returndata before Byzantium
        return data.length;
    }
}
