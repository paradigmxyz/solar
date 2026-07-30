//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract MemoryFixedArrayAlloc {
    struct Holder {
        uint256[3] values;
    }

    // CHECK-LABEL: fn @guardedFix{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: lt arg0, 3
    // CHECK: mstore 4, 50
    // CHECK: memory_object_element_addr memoryfixedarray<3, 1>, [[ARRAY]], arg0
    function guardedFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        return x[i];
    }

    // CHECK-LABEL: fn @structArr{{[( ]}}
    // CHECK: [[STRUCT:v[0-9]+]] = alloc memorystruct<1>
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: memory_object_field_addr memorystruct<1>, [[STRUCT]], 0
    // CHECK: memory_object_element_addr memoryfixedarray<3, 1>, {{v[0-9]+}}, arg0
    function structArr(uint256 i) public pure returns (uint256) {
        Holder memory h;
        return h.values[i];
    }

    // CHECK-LABEL: fn @nested{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK-COUNT-3: alloc memoryfixedarray<2, 1>
    // CHECK: memory_object_element_addr memoryfixedarray<3, 1>, [[OUTER]], arg0
    // CHECK: memory_object_element_addr memoryfixedarray<2, 1>, {{v[0-9]+}}, arg1
    function nested(uint256 i, uint256 j) public pure returns (uint256) {
        uint256[2][3] memory x;
        x[0][0] = 1;
        return x[i][j];
    }

    // CHECK-LABEL: fn @fmpIntegrity{{[( ]}}
    // CHECK: {{v[0-9]+}} = alloc memoryfixedarray<3, 1>
    // CHECK: mstore {{v[0-9]+}}, 7
    // CHECK: {{v[0-9]+}} = alloc memoryarray<1>
    // CHECK: mstore {{v[0-9]+}}, 9
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}
    function fmpIntegrity() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[2] = 7;
        uint256[] memory y = new uint256[](1);
        y[0] = 9;
        return (x[2], y[0]);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: [[FIRST:v[0-9]+]] = add [[ARRAY]], 0
    // CHECK: mstore [[FIRST]], 1
    // CHECK: mstore {{v[0-9]+}}, 2
    // CHECK: mstore {{v[0-9]+}}, 3
    function literal() public pure returns (uint256) {
        uint256[3] memory x = [uint256(1), uint256(2), uint256(3)];
        return x[2];
    }

    // Wide value arrays use one semantic bulk-zero operation.
    // CHECK-LABEL: fn @bulkDefault{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<4, 1>
    // CHECK: memory_zero [[ARRAY]], 128
    // CHECK: memory_object_element_addr memoryfixedarray<4, 1>, [[ARRAY]], arg0
    function bulkDefault(uint256 i) public pure returns (uint256) {
        uint256[4] memory x;
        return x[i];
    }
}

contract NamedReturnAndDelete {
    struct DynamicHolder {
        uint256[] values;
        bytes data;
    }

    struct WideHolder {
        uint256 first;
        bytes data;
        uint256 third;
        uint256 fourth;
    }

    // A named fixed-array return points at real zeroed memory, not scratch.
    // CHECK-LABEL: fn @namedReturn{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: mstore {{v[0-9]+}}, 1
    // CHECK: mstore {{v[0-9]+}}, 3
    // CHECK: {{v[0-9]+}} = alloc memorybytes
    // CHECK: mstore8 {{v[0-9]+}}, 238
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}
    function namedReturn() public pure returns (uint256[3] memory x, uint256 m) {
        x[0] = 1;
        x[2] = 3;
        bytes memory b = new bytes(32);
        b[0] = 0xEE;
        m = uint8(b[0]);
    }

    // Uninitialized memory references point at real empty objects, not scratch.
    // CHECK-LABEL: fn @emptyMemoryReferences{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: set_memory_object_len memoryarray, [[ARRAY]], 0
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[BYTES]], 0
    // CHECK: [[STRUCT:v[0-9]+]] = alloc memorystruct<2>
    // CHECK: alloc memoryarray<1>
    // CHECK: alloc memorybytes
    function emptyMemoryReferences() public pure returns (uint256) {
        uint256[] memory values;
        bytes memory data;
        DynamicHolder memory holder;
        return values.length + data.length + holder.values.length + holder.data.length;
    }

    // Named dynamic returns also start as real empty memory objects.
    // CHECK-LABEL: fn @emptyNamedReturns{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryarray<1>
    // CHECK: set_memory_object_len memoryarray, [[ARRAY]], 0
    // CHECK: [[BYTES:v[0-9]+]] = alloc memorybytes
    // CHECK: set_memory_object_len memorybytes, [[BYTES]], 0
    function emptyNamedReturns()
        public
        pure
        returns (uint256[] memory values, bytes memory data)
    {}

    // A single wide named struct return zeroes scalar fields in bulk while
    // reference fields still point at real empty objects.
    // CHECK-LABEL: fn @emptyWideNamedStruct{{[( ]}}
    // CHECK: [[WIDE:v[0-9]+]] = alloc memorystruct<4>, exact, uninitialized, infallible, 128
    // CHECK: memory_zero [[WIDE]], 128
    // CHECK: [[EMPTY:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 32
    // CHECK: set_memory_object_len memorybytes, [[EMPTY]], 0
    // CHECK: [[DATA:v[0-9]+]] = memory_object_field_addr memorystruct<4>, [[WIDE]], 1
    // CHECK: mstore [[DATA]], [[EMPTY]]
    // CHECK-NOT: mstore {{v[0-9]+}}, 0
    // CHECK: mstore 128, [[WIDE]]
    function emptyWideNamedStruct() public pure returns (WideHolder memory holder) {}

    // Named struct returns always receive semantic default objects; optimization
    // passes remove stores overwritten before reads.
    // CHECK-LABEL: fn @fullyInitializedNamedStruct{{[( ]}}
    // CHECK: alloc memorystruct<2>
    // CHECK: set_memory_object_len memoryarray, {{v[0-9]+}}, 0
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 0
    // CHECK: set_memory_object_len memoryarray, {{v[0-9]+}}, 1
    // CHECK: set_memory_object_len memorybytes, {{v[0-9]+}}, 1
    function fullyInitializedNamedStruct()
        public
        pure
        returns (DynamicHolder memory holder)
    {
        holder.values = new uint256[](1);
        holder.data = new bytes(1);
    }

    // `delete` zeroes the elements in place; the pointer stays valid.
    // CHECK-LABEL: fn @deleteInPlace{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<3, 1>
    // CHECK: mstore [[ARRAY]], 0
    // CHECK: [[SECOND:v[0-9]+]] = add [[ARRAY]], 32
    // CHECK: mstore [[SECOND]], 0
    // CHECK: [[THIRD:v[0-9]+]] = add [[ARRAY]], 64
    // CHECK: mstore [[THIRD]], 0
    // CHECK: mstore {{v[0-9]+}}, 9
    function deleteInPlace() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[0] = 5;
        x[1] = 6;
        x[2] = 7;
        delete x;
        x[2] = 9;
        return (x[0], x[2]);
    }

    // Deleting a wide value array also zeroes it in bulk.
    // CHECK-LABEL: fn @bulkDeleteInPlace{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc memoryfixedarray<4, 1>
    // CHECK: mstore {{v[0-9]+}}, 7
    // CHECK: memory_zero [[ARRAY]], 128
    // CHECK: mstore {{v[0-9]+}}, 9
    function bulkDeleteInPlace() public pure returns (uint256, uint256) {
        uint256[4] memory x;
        x[0] = 5;
        x[3] = 7;
        delete x;
        x[3] = 9;
        return (x[0], x[3]);
    }
}
