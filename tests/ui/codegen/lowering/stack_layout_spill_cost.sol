//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

struct Market {
    uint128 fee;
}

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

    function checkDynArray() external returns (uint256) { //~ WARN: unnamed return variable can remain unassigned
        arr.push(11);
        uint256 slot;
        assembly {
            slot := arr.slot
        }
        uint256 data = uint256(keccak256(abi.encode(slot)));
        require(raw(4) == 3, "len");
        require(raw(data) == 7 | (uint256(9) << 128), "data0");
        require(raw(data + 1) == 11, "data1");
        require(arr[0] == 7 && arr[1] == 9 && arr[2] == 11, "elems");
        arr[1] = 13;
        require(raw(data) == 7 | (uint256(13) << 128), "elem write");
    }

    function checkSignedArray() external view returns (uint256) { //~ WARN: unnamed return variable can remain unassigned
        require(signedArr[0] == -5 && signedArr[1] == 6, "signed reads");
    }

    // The dynamic data slot stays on the stack across the length check rather
    // than paying for a frame store and reload. The constant-slot case above
    // can fold its hash before stack scheduling.
    // CHECK: push 0x4ebcfdd1
    // CHECK-NEXT: eq
    // CHECK-NEXT: push [[BODY:bb[0-9]+]]
    // CHECK: [[BODY]]:
    // CHECK: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: keccak256
    // CHECK-NOT: mstore
    // CHECK: dup 1
    // CHECK-NEXT: sload
    function checkDynamicSlot(uint256 slot) external view returns (uint256) {
        uint256 data = uint256(keccak256(abi.encode(slot)));
        require(raw(4) == 3, "len");
        require(raw(data) == 7 | (uint256(9) << 128), "data0");
        return raw(data + 1);
    }
}
