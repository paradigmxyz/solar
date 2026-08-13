//@compile-flags: -O none -Zdump=mir

contract CalldataArraySubsliceDynamic {
    // A sub-slice of a dynamic-element array keeps element offsets relative to
    // the original base, which a rebuild cannot recover, so it is rejected
    // rather than miscompiled.
    function dynamic(bytes[] calldata data) external pure returns (bytes[] memory) {
        return data[1:]; //~ ERROR: codegen rewrite does not support this slice yet
    }
}
