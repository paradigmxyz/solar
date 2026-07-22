//@compile-flags: -Zcodegen -Zdump=mir
//@check-fail

contract CalldataArraySubsliceDynamic {
    // A sub-slice of a dynamic-element array keeps element offsets relative to
    // the original base, which a rebuild cannot recover, so it is rejected
    // rather than miscompiled.
    function dynamic(bytes[] calldata data) external pure returns (bytes[] memory) {
        return data[1:]; //~ ERROR: codegen does not support slicing a calldata array of dynamic elements yet
        //~^ ERROR: codegen does not support slicing a calldata array of dynamic elements yet
    }
}
