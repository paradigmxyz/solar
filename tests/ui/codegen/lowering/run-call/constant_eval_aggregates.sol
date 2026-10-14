//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: structs => 0x46700b4d40ac5c35af2c22dda2787a91eb567b06c924a8fb8ae9a05b20c08c21, 0x5a4be9052f25ff6d33da572b95cd7cd5a1b35d87e5353835fb359783cff622f9
//@ run-call: arrays => 0x6cce5286d246c8ed7cf3fd3b7e50a798e684660929923ac369039151ef8a27fc, 0x7cde746dfbb8dfd7721b5995769f873e3ff50416302673a354990b553bb0e208
//@ run-call: zeroBytes => 3, 0x99ff0d9125e1fc9531a11262e15aeb2c60509a078c4cc4c64cefdfb06ff68647
//@ run-call: encodeCall => 0x3c916696a0faa1db2286dbb50567e1cca25ef734021b736074d7fb8ec68a4318
//@ run-call: libraryConstant => 1000000

// Constant evaluation folds structs, `new` arrays and byte arrays, their ABI encodings,
// `abi.encodeCall`, and public constants of libraries, so each of these functions returns
// immediates. The expected values come from `cast`.

enum ItemType {
    A,
    B
}

struct Item {
    ItemType itemType;
    address token;
    uint256 amount;
}

struct Params {
    address offerer;
    Item[] items;
    bytes32 zoneHash;
    bytes extra;
}

struct Pair {
    uint8[2] pair;
    bytes extra;
}

interface I {
    function f(uint256, string calldata) external;
}

library L {
    uint24 public constant MAX = 1000000;
}

contract ConstantEvalAggregates {
    bytes32 constant EMPTY_ITEM =
        keccak256(abi.encode(Item({itemType: ItemType(0), token: address(0), amount: 0})));
    bytes32 constant EMPTY_PARAMS = keccak256(
        abi.encode(
            Params({offerer: address(0), items: new Item[](0), zoneHash: bytes32(0), extra: ""})
        )
    );
    bytes32 constant TWO_ITEMS = keccak256(abi.encode(new Item[](2), Item(ItemType.B, address(1), 3)));
    bytes32 constant ZERO_PAIR = keccak256(abi.encode(new Pair[](1)));
    bytes constant ZERO = new bytes(3);
    bytes32 constant CALL = keccak256(abi.encodeCall(I.f, (1, "ab")));
    uint24 constant MAX = L.MAX;

    // CHECK-LABEL: fn @structs()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function structs() external pure returns (bytes32, bytes32) {
        return (EMPTY_ITEM, EMPTY_PARAMS);
    }

    // CHECK-LABEL: fn @arrays()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function arrays() external pure returns (bytes32, bytes32) {
        return (TWO_ITEMS, ZERO_PAIR);
    }

    function zeroBytes() external pure returns (uint256, bytes32) {
        return (ZERO.length, keccak256(ZERO));
    }

    // CHECK-LABEL: fn @encodeCall()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function encodeCall() external pure returns (bytes32) {
        return CALL;
    }

    // CHECK-LABEL: fn @libraryConstant()
    // CHECK: ret i24 0xf4240
    function libraryConstant() external pure returns (uint24) {
        return MAX;
    }
}
