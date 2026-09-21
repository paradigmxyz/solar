//@ codegen-matrix: standard

contract OversizedStorageCopy {
    uint256[1 << 64] private source;
    uint256[1 << 64] private target;

    function copy() external {
        target = source; //~ ERROR: codegen rewrite does not support this oversized fixed-array materialization yet
    }

    function tupleCopy() external {
        (target, target) = (source, source);
        //~^ ERROR: codegen rewrite does not support this oversized fixed-array materialization yet
    }
}
