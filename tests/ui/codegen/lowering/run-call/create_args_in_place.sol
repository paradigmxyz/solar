//@ revisions: cancun shanghai shanghaiSize
//@[cancun] compile-flags: -O gas --evm-version cancun
//@[shanghai] compile-flags: -O gas --evm-version shanghai
//@[shanghaiSize] compile-flags: -O size --evm-version shanghai
//@ run-call: staticArguments => 0, 11, 7
//@ run-call: dynamicBytes => 0, 1000, 5
//@ run-call: dynamicMixed => 0, 40, 3, 6
//@ run-call: dynamicArray => 0, 4, 10
//@ run-call: salted => 0, 11, 7
//@ run-call: saltedDynamic => 0, 5, 5
//@ run-call: noArguments => 0, 3
//@ run-call: repeated => 0, 11, 13

// The constructor arguments of `new C(...)` are encoded straight after the initcode at the free
// memory pointer, as solc does, so a creation moves the pointer by nothing whether the arguments
// are static, dynamic or absent, with or without a salt, and with or without `MCOPY`.
contract Static {
    uint256 public sum;
    uint256 public tag;

    constructor(uint256 a, uint256 b, uint256 tag_) {
        sum = a + b;
        tag = tag_;
    }
}

contract DynamicBytes {
    uint256 public length;
    uint256 public last;

    constructor(bytes memory data) {
        length = data.length;
        last = uint8(data[data.length - 1]);
    }
}

contract DynamicMixed {
    uint256 public length;
    uint256 public first;
    uint256 public extra;

    constructor(uint256 first_, string memory text, uint256 extra_) {
        length = bytes(text).length;
        first = first_;
        extra = extra_;
    }
}

contract DynamicArray {
    uint256 public length;
    uint256 public sum;

    constructor(uint256[] memory values) {
        length = values.length;
        for (uint256 i = 0; i < values.length; ++i) {
            sum += values[i];
        }
    }
}

contract Empty {
    uint256 public value = 3;
}

contract CreateArgsInPlace {
    function fmp() internal pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
        }
    }

    function staticArguments() external returns (uint256 moved, uint256 sum, uint256 tag) {
        uint256 before = fmp();
        Static created = new Static(5, 6, 7);
        moved = fmp() - before;
        return (moved, created.sum(), created.tag());
    }

    function dynamicBytes() external returns (uint256 moved, uint256 length, uint256 last) {
        bytes memory data = new bytes(1000);
        data[999] = bytes1(uint8(5));
        uint256 before = fmp();
        DynamicBytes created = new DynamicBytes(data);
        moved = fmp() - before;
        return (moved, created.length(), created.last());
    }

    function dynamicMixed() external returns (uint256 moved, uint256 length, uint256 first, uint256 extra) {
        string memory text = string(new bytes(40));
        uint256 before = fmp();
        DynamicMixed created = new DynamicMixed(3, text, 6);
        moved = fmp() - before;
        return (moved, created.length(), created.first(), created.extra());
    }

    function dynamicArray() external returns (uint256 moved, uint256 length, uint256 sum) {
        uint256[] memory values = new uint256[](4);
        for (uint256 i = 0; i < 4; ++i) {
            values[i] = i + 1;
        }
        uint256 before = fmp();
        DynamicArray created = new DynamicArray(values);
        moved = fmp() - before;
        return (moved, created.length(), created.sum());
    }

    function salted() external returns (uint256 moved, uint256 sum, uint256 tag) {
        uint256 before = fmp();
        Static created = new Static{salt: bytes32(uint256(1))}(5, 6, 7);
        moved = fmp() - before;
        return (moved, created.sum(), created.tag());
    }

    function saltedDynamic() external returns (uint256 moved, uint256 length, uint256 last) {
        bytes memory data = new bytes(5);
        data[4] = bytes1(uint8(5));
        uint256 before = fmp();
        DynamicBytes created = new DynamicBytes{salt: bytes32(uint256(2))}(data);
        moved = fmp() - before;
        return (moved, created.length(), created.last());
    }

    function noArguments() external returns (uint256 moved, uint256 value) {
        uint256 before = fmp();
        Empty created = new Empty();
        moved = fmp() - before;
        return (moved, created.value());
    }

    function repeated() external returns (uint256 moved, uint256 first, uint256 second) {
        uint256 before = fmp();
        first = new Static(5, 6, 7).sum();
        second = new Static(6, 7, 8).sum();
        moved = fmp() - before;
    }
}
