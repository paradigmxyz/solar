//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => true
// ported-from: test/libsolidity/semanticTests/array/copying/array_copy_cleanup_uint40.sol

contract ArrayCopyCleanupUint40 {
    uint40[] x;

    function f() public returns (bool) {
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);
        x.push(42);

        uint40[] memory y = new uint40[](1);
        y[0] = 23;
        x = y;

        assembly {
            sstore(x.slot, 20)
        }

        assert(x[0] == 23);
        assert(x[1] == 0);
        assert(x[2] == 0);
        assert(x[3] == 0);
        assert(x[4] == 0);
        assert(x[5] == 0);
        assert(x[6] == 0);
        assert(x[7] == 0);
        assert(x[8] == 0);
        assert(x[9] == 0);
        assert(x[10] == 0);
        assert(x[11] == 0);
        assert(x[12] == 0);
        assert(x[13] == 0);
        assert(x[14] == 0);
        assert(x[15] == 0);
        assert(x[16] == 0);
        assert(x[17] == 0);
        assert(x[18] == 0);
        assert(x[19] == 0);

        return true;
    }
}
