//@revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck:

contract Caller {
    // CHECK-LABEL: fn @probe
    // CHECK: staticcall
    function probe(address target) external view returns (bool) {
        (bool success,) = target.staticcall("");
        //~[homestead]^ ERROR: builtin `staticcall` requires Byzantium-compatible EVM
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
