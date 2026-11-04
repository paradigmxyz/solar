//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::noop => 0x08c379a000000000000000000000000000000000000000000000000000000000
//@ run-call: C::shifted => 32
//@ run-call: C::short => 0xaabbcc0000000000000000000000000000000000000000000000000000000000
//@ run-call: C::weirollMessage => 0x496e73756666696369656e74206f757470757400000000000000000000000000
//@ run-call: C::length => 3
//@ run-call: C::index => 0xaa
//@ run-call: C::hash => true
//@ run-call: C::encoded => true
//@ run-call: C::text => true
//@ run-call: C::returned => 0xaabbcc
//@ run-call: C::structField => 2
//@ run-call: C::shiftedStruct => 2
//@ run-call: C::arrayElement => 20
//@ run-call: C::arrayLength => 2
//@ run-call: C::calldataBytes 0xaabbcc => 0xaabbcc
//@ run-call: C::calldataOffsetFromMemory 0xaabbcc => 3
//@ run-call: C::calldataLengthFromMemory 0xaabbcc => 2

// A memory reference variable assigned in inline assembly stays a memory reference: every
// reader sees the object the word points to, not the word. Weiroll's VM decodes revert reasons
// by shifting a `bytes` pointer over the selector and the offset word, then converting it to
// `bytes32`.

contract C {
    struct S {
        uint256 a;
        uint256 b;
    }

    function data() internal pure returns (bytes memory) {
        return abi.encodeWithSignature("Error(string)", "Insufficient output");
    }

    function noop() external pure returns (bytes32) {
        bytes memory b = data();
        assembly {
            b := b
        }
        return bytes32(b);
    }

    // CHECK-LABEL: fn @shifted()
    // CHECK: [[WORD:v[0-9]+]] = add
    // CHECK-NEXT: [[B:v[0-9]+]] = inttoptr i256 [[WORD]] to ptr<memory>
    // CHECK: mload [[B]]
    function shifted() external pure returns (uint256) {
        bytes memory b = data();
        assembly {
            b := add(b, 4)
        }
        return uint256(bytes32(b));
    }

    function short() external pure returns (bytes32) {
        bytes memory b = hex"aabbcc";
        assembly {
            b := b
        }
        return bytes32(b);
    }

    function weirollMessage() external pure returns (bytes32) {
        bytes memory outData = data();
        assembly {
            outData := add(outData, 36)
        }
        uint256 size = uint256(bytes32(outData));
        require(size == 19, "size");
        assembly {
            outData := add(outData, 32)
        }
        return bytes32(bytes(string(outData)));
    }

    function length() external pure returns (uint256) {
        bytes memory b = hex"aabbcc";
        assembly {
            b := b
        }
        return b.length;
    }

    function index() external pure returns (bytes1) {
        bytes memory b = hex"aabbcc";
        assembly {
            b := b
        }
        return b[0];
    }

    function hash() external pure returns (bool) {
        bytes memory b = hex"aabbcc";
        assembly {
            b := b
        }
        return keccak256(b) == keccak256(hex"aabbcc");
    }

    function encoded() external pure returns (bool) {
        bytes memory b = hex"aabbcc";
        bytes memory c = hex"aabbcc";
        assembly {
            b := b
        }
        return keccak256(abi.encode(b)) == keccak256(abi.encode(c));
    }

    function text() external pure returns (bool) {
        bytes memory b = "abc";
        assembly {
            b := b
        }
        string memory s = string(b);
        return keccak256(bytes(s)) == keccak256("abc");
    }

    function pass(bytes memory b) internal pure returns (bytes memory) {
        assembly {
            b := b
        }
        return b;
    }

    function returned() external pure returns (bytes memory) {
        return pass(hex"aabbcc");
    }

    function structField() external pure returns (uint256) {
        S memory s = S(1, 2);
        assembly {
            s := s
        }
        return s.b;
    }

    function shiftedStruct() external pure returns (uint256) {
        S memory s = S(1, 2);
        assembly {
            s := add(s, 32)
        }
        return s.a;
    }

    function arrayElement() external pure returns (uint256) {
        uint256[] memory a = new uint256[](2);
        a[0] = 10;
        a[1] = 20;
        assembly {
            a := a
        }
        return a[1];
    }

    function arrayLength() external pure returns (uint256) {
        uint256[] memory a = new uint256[](2);
        assembly {
            a := a
        }
        return a.length;
    }

    function calldataBytes(bytes calldata c) external pure returns (bytes memory) {
        assembly {
            c.offset := c.offset
        }
        return c;
    }

    function calldataOffsetFromMemory(bytes calldata c) external pure returns (uint256) {
        bytes memory b = new bytes(1);
        assembly {
            b := c.offset
        }
        assembly {
            c.offset := b
        }
        return c.length;
    }

    function calldataLengthFromMemory(bytes calldata c) external pure returns (uint256) {
        bytes memory b = new bytes(1);
        assembly {
            b := 2
        }
        assembly {
            c.length := b
        }
        return c.length;
    }
}
