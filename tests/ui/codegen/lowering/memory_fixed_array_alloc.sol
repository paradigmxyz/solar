//@ compile-flags: -O none -Zdump=mir
//@filecheck:

contract MemoryFixedArrayAlloc {
    struct Holder {
        uint256[3] values;
    }

    // CHECK-LABEL: fn @guardedFix{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 96
    // CHECK: lt arg0, 3
    // CHECK: icall panic_if<0x32>, {{v[0-9]+}}
    // CHECK-NEXT: [[OFF1:v[0-9]+]] = shl 5, arg0
    // CHECK-NEXT: [[ELEM1:v[0-9]+]] = ptradd [[ARRAY]], [[OFF1]]
    // CHECK-NEXT: mload [[ELEM1]]
    function guardedFix(uint256 i) public pure returns (uint256) {
        uint256[3] memory x;
        return x[i];
    }

    // CHECK-LABEL: fn @structArr{{[( ]}}
    // CHECK: [[STRUCT:v[0-9]+]] = alloc 32
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 96
    // CHECK: mstore [[STRUCT]], {{v[0-9]+}}
    // CHECK-NEXT: [[VALUES:v[0-9]+]] = mload [[STRUCT]], ptr<memory>
    // CHECK: [[OFF2:v[0-9]+]] = shl 5, arg0
    // CHECK-NEXT: [[ELEM2:v[0-9]+]] = ptradd [[VALUES]], [[OFF2]]
    // CHECK-NEXT: mload [[ELEM2]]
    function structArr(uint256 i) public pure returns (uint256) {
        Holder memory h;
        return h.values[i];
    }

    // CHECK-LABEL: fn @nested{{[( ]}}
    // CHECK: [[OUTER:v[0-9]+]] = alloc 96
    // CHECK: [[INDEX:v[0-9]+]] = phi
    // CHECK: [[OFF3:v[0-9]+]] = shl 5, arg0
    // CHECK-NEXT: [[ROW:v[0-9]+]] = ptradd [[OUTER]], [[OFF3]]
    // CHECK-NEXT: mload [[ROW]]
    // CHECK: [[OFF4:v[0-9]+]] = shl 5, arg1
    // CHECK-NEXT: [[CELL:v[0-9]+]] = ptradd {{v[0-9]+}}, [[OFF4]]
    // CHECK-NEXT: mload [[CELL]]
    // CHECK: [[INNER:v[0-9]+]] = alloc 64
    // CHECK-NEXT: memory_zero [[INNER]], 64
    // CHECK: [[OFF5:v[0-9]+]] = shl 5, [[INDEX]]
    // CHECK-NEXT: [[SLOT:v[0-9]+]] = ptradd [[OUTER]], [[OFF5]]
    // CHECK-NEXT: [[INNER_WORD:v[0-9]+]] = ptrtoint ptr<memory> [[INNER]] to i256
    // CHECK-NEXT: mstore [[SLOT]], [[INNER_WORD]]
    function nested(uint256 i, uint256 j) public pure returns (uint256) {
        uint256[2][3] memory x;
        x[0][0] = 1;
        return x[i][j];
    }

    // CHECK-LABEL: fn @fmpIntegrity{{[( ]}}
    // CHECK: [[X:v[0-9]+]] = alloc 96
    // CHECK: mstore [[X]]+64, 7
    // CHECK: [[Y:v[0-9]+]] = alloc
    // CHECK: mstore [[Y]]+32, 9
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, {{v[0-9]+}}
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: ret [[RET_1]]
    function fmpIntegrity() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[2] = 7;
        uint256[] memory y = new uint256[](1);
        y[0] = 9;
        return (x[2], y[0]);
    }

    // CHECK-LABEL: fn @literal{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 96
    // CHECK-NEXT: mstore [[ARRAY]], 1
    // CHECK-NEXT: mstore [[ARRAY]]+32, 2
    // CHECK-NEXT: mstore [[ARRAY]]+64, 3
    function literal() public pure returns (uint256) {
        uint256[3] memory x = [uint256(1), uint256(2), uint256(3)];
        return x[2];
    }

    // Wide value arrays use one semantic bulk-zero operation.
    // CHECK-LABEL: fn @bulkDefault{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 128
    // CHECK: memory_zero {{v[0-9]+}}, 128
    // CHECK: [[OFF6:v[0-9]+]] = shl 5, arg0
    // CHECK-NEXT: [[ELEM6:v[0-9]+]] = ptradd [[ARRAY]], [[OFF6]]
    // CHECK-NEXT: mload [[ELEM6]]
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
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 96
    // CHECK: mstore [[ARRAY]], 1
    // CHECK: mstore [[ARRAY]]+64, 3
    // CHECK: [[BYTES:v[0-9]+]] = alloc
    // CHECK: mstore8 [[BYTES]]+32, {{v[0-9]+}}
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, {{v[0-9]+}}
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: ret [[RET_1]]
    function namedReturn() public pure returns (uint256[3] memory x, uint256 m) {
        x[0] = 1;
        x[2] = 3;
        bytes memory b = new bytes(32);
        b[0] = 0xEE;
        m = uint8(b[0]);
    }

    // Uninitialized dynamic references use Solidity's zero slot.
    // CHECK-LABEL: fn @emptyMemoryReferences{{[( ]}}
    // CHECK: [[STRUCT:v[0-9]+]] = alloc 64
    // CHECK-NEXT: mstore [[STRUCT]], 96
    // CHECK-NEXT: mstore [[STRUCT]]+32, 96
    // CHECK-NEXT: {{v[0-9]+}} = mload ptr<memory> 96
    // CHECK-NEXT: {{v[0-9]+}} = mload ptr<memory> 96
    function emptyMemoryReferences() public pure returns (uint256) {
        uint256[] memory values;
        bytes memory data;
        DynamicHolder memory holder;
        return values.length + data.length + holder.values.length + holder.data.length;
    }

    // Named dynamic returns also start at the zero slot.
    // CHECK-LABEL: fn @emptyNamedReturns{{[( ]}}
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, ptr<memory> 96
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, ptr<memory> 96
    // CHECK: ret [[RET_1]]
    function emptyNamedReturns()
        public
        pure
        returns (uint256[] memory values, bytes memory data)
    {}

    // Wide named struct defaults bulk-zero scalars and initialize references.
    // CHECK-LABEL: fn @emptyWideNamedStruct{{[( ]}}
    // CHECK: [[WIDE:v[0-9]+]] = alloc 128
    // CHECK: memory_zero [[WIDE]], 128
    // CHECK-NEXT: mstore [[WIDE]]+32, 96
    // CHECK: ret [[WIDE]]
    function emptyWideNamedStruct() public pure returns (WideHolder memory holder) {}

    // Named struct returns always receive semantic default objects; optimization
    // passes remove stores overwritten before reads.
    // CHECK-LABEL: fn @fullyInitializedNamedStruct{{[( ]}}
    // CHECK: [[STRUCT:v[0-9]+]] = alloc 64
    // CHECK-NEXT: mstore [[STRUCT]], 96
    // CHECK-NEXT: mstore [[STRUCT]]+32, 96
    // CHECK: [[BYTES:v[0-9]+]] = alloc
    // CHECK: mstore [[BYTES]], 1
    function fullyInitializedNamedStruct()
        public
        pure
        returns (DynamicHolder memory holder)
    {
        holder.values = new uint256[](1);
        holder.data = new bytes(1);
    }

    // `delete` rebinds the reference to a fresh zeroed object.
    // CHECK-LABEL: fn @deleteRebind{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 96
    // CHECK: mstore [[ARRAY]], 5
    // CHECK: mstore [[ARRAY]]+32, 6
    // CHECK: mstore [[ARRAY]]+64, 7
    // CHECK: [[DELETED:v[0-9]+]] = alloc 96
    // CHECK: mstore [[DELETED]]+64, 9
    function deleteRebind() public pure returns (uint256, uint256) {
        uint256[3] memory x;
        x[0] = 5;
        x[1] = 6;
        x[2] = 7;
        delete x;
        x[2] = 9;
        return (x[0], x[2]);
    }

    // Deleting a wide value array also allocates a fresh zeroed object.
    // CHECK-LABEL: fn @bulkDeleteRebind{{[( ]}}
    // CHECK: [[ARRAY:v[0-9]+]] = alloc 128
    // CHECK: mstore [[ARRAY]]+96, 7
    // CHECK: [[DELETED:v[0-9]+]] = alloc 128
    // CHECK: mstore [[DELETED]]+96, 9
    function bulkDeleteRebind() public pure returns (uint256, uint256) {
        uint256[4] memory x;
        x[0] = 5;
        x[3] = 7;
        delete x;
        x[3] = 9;
        return (x[0], x[3]);
    }
}
