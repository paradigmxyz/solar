contract CalldataArraySubsliceDynamic {
    // Range access on arrays with dynamically encoded base types is rejected
    // by solc itself, so the compiler matches that semantics instead of
    // carrying a codegen bail.
    function dynamic(bytes[] calldata data) external pure returns (bytes[] memory) {
        return data[1:]; //~ ERROR: index range access is not supported for arrays with dynamically encoded base types
    }
}
