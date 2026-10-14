//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: digest 128, 1, 2, 3, 4 => 0x392791df626408017a264f53fde61065d5a93a32b60171df9d8a46afdf82992d

// CHECK-LABEL: fn @digest
// CHECK: mstore [[BASE:arg[0-9]+]], arg1
// CHECK: [[FIRST:v[0-9]+]] = add [[BASE]], 32
// CHECK: mstore [[FIRST]], arg2
// CHECK: [[SECOND:v[0-9]+]] = add [[FIRST]], 32
// CHECK: mstore [[SECOND]], arg3
// CHECK: [[THIRD:v[0-9]+]] = add [[SECOND]], 32
// CHECK: mstore [[THIRD]], arg4
contract Test {
    function digest(uint256 x, uint256 a, uint256 b, uint256 c, uint256 d)
        external
        pure
        returns (bytes32 result)
    {
        assembly {
            mstore(x, a)
            mstore(add(x, 32), b)
            mstore(add(x, 64), c)
            mstore(add(x, 96), d)
            result := keccak256(x, 128)
        }
    }
}
