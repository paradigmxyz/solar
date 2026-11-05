//@revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck:

contract Caller {
    // CHECK-LABEL: fn @probeCall
    // CHECK: address_call
    // CHECK: icall returndata_bytes<>
    function probeCall(address target) external returns (uint256) {
        (, bytes memory data) = target.call("");
        //~[homestead]^ ERROR: codegen cannot bind low-level call returndata before Byzantium
        return data.length;
    }
}
