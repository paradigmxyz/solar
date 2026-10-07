//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:

struct Market {
    uint128 fee;
}

// The array data slot stays on the stack across the length checks. A frame
// spill would pay for a store, the copy that feeds it, and a later reload, so
// the layout keeps the word resident and the slot feeds `sload` directly.
// CHECK: push 11
// CHECK: keccak256
// CHECK-NEXT: swap 1
// CHECK-NEXT: push 3
// CHECK-NEXT: sub
// CHECK-NEXT: push bb{{[0-9]+}}
// CHECK-NEXT: jumpi
// CHECK-NEXT: dup 1
// CHECK-NEXT: sload
// CHECK-NEXT: push 0x900000000000000000000000000000007
contract SpillCost {
    uint128 public a;
    bool public c;
    int64 public d;
    bytes8 public e;
    uint256 public f;
    mapping(bytes32 => Market) public market;
    uint128[] public arr;
    int32[] public signedArr;

    function raw(uint256 slot) internal view returns (uint256 out) {
        assembly {
            out := sload(slot)
        }
    }

    function checkDynArray() external returns (uint256) {
        arr.push(11);
        uint256 data = uint256(keccak256(abi.encode(uint256(4))));
        require(raw(4) == 3, "len");
        require(raw(data) == 7 | (uint256(9) << 128), "data0");
        require(raw(data + 1) == 11, "data1");
        require(arr[0] == 7 && arr[1] == 9 && arr[2] == 11, "elems");
        arr[1] = 13;
        require(raw(data) == 7 | (uint256(13) << 128), "elem write");
    }

    function checkSignedArray() external view returns (uint256) {
        require(signedArr[0] == -5 && signedArr[1] == 6, "signed reads");
    }
}
