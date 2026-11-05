//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: copy [11, 22, 33, 44] => 1, 11, 2, 33

// An inline array literal copied into storage seeds its elements with the
// destination's storage element type, which no calldata slice unifies with.
// The literal then falls back to the element's own mobile type, so the slices
// mobilize to memory arrays and are copied into storage element-wise.
contract StorageArrayLiteralSlices {
    uint256[][2] private stored;

    function copy(uint256[] calldata values)
        external
        returns (uint256, uint256, uint256, uint256)
    {
        stored = [values[0:1], values[1:3]];
        return (stored[0].length, stored[0][0], stored[1].length, stored[1][1]);
    }
}
