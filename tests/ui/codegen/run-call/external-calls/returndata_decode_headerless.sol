//@ codegen-matrix: standard
//@ run-call: directBytes => 224, 33, 0x61
//@ run-call: pointerBytes => 224, 33, 0x61
//@ run-call: tryBytes => 224, 33, 0x61
//@ run-call: directEmpty => 96, 0
//@ run-call: directMixed => 7, 33
//@ run-call: directUnaligned => 1, 0x61
//@ run-call-fail: directTruncated

// The return data of a dynamic return is copied into a buffer with no length word, sized to the
// padded return data, and decoded in place. The growth of the free pointer is that buffer plus the
// decoded value's own object: a 33-byte string returns 128 bytes (head, length, two payload words)
// and decodes into 96 bytes (length and two payload words).

interface Callee {
    function bytesOf(uint256 n) external pure returns (bytes memory);
    function mixed() external pure returns (uint256, bytes memory);
}

contract Impl {
    function bytesOf(uint256 n) external pure returns (bytes memory result) {
        result = new bytes(n);
        if (n != 0) result[0] = 0x61;
    }

    function mixed() external pure returns (uint256, bytes memory) {
        return (7, new bytes(33));
    }
}

contract Truncated {
    fallback() external {
        assembly {
            mstore(0, 0x20)
            mstore(32, 64)
            return(0, 64)
        }
    }
}

contract Unaligned {
    fallback() external {
        assembly {
            mstore(0, 0x20)
            mstore(32, 1)
            mstore8(64, 0x61)
            return(0, 65)
        }
    }
}

contract ReturndataDecodeHeaderless {
    function directBytes() external returns (uint256 allocated, uint256 length, uint8 first) {
        Callee target = Callee(address(new Impl()));
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        bytes memory result = target.bytesOf(33);
        assembly {
            allocated := sub(mload(0x40), before_)
        }
        length = result.length;
        first = uint8(result[0]);
    }

    function pointerBytes() external returns (uint256 allocated, uint256 length, uint8 first) {
        Callee target = Callee(address(new Impl()));
        function(uint256) external pure returns (bytes memory) call_ = target.bytesOf;
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        bytes memory result = call_(33);
        assembly {
            allocated := sub(mload(0x40), before_)
        }
        length = result.length;
        first = uint8(result[0]);
    }

    function tryBytes() external returns (uint256 allocated, uint256 length, uint8 first) {
        Callee target = Callee(address(new Impl()));
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        try target.bytesOf(33) returns (bytes memory result) {
            assembly {
                allocated := sub(mload(0x40), before_)
            }
            length = result.length;
            first = uint8(result[0]);
        } catch {
            revert();
        }
    }

    function directEmpty() external returns (uint256 allocated, uint256 length) {
        Callee target = Callee(address(new Impl()));
        uint256 before_;
        assembly {
            before_ := mload(0x40)
        }
        bytes memory result = target.bytesOf(0);
        assembly {
            allocated := sub(mload(0x40), before_)
        }
        length = result.length;
    }

    function directMixed() external returns (uint256 number, uint256 length) {
        bytes memory result;
        (number, result) = Callee(address(new Impl())).mixed();
        length = result.length;
    }

    function directUnaligned() external returns (uint256 length, uint8 first) {
        bytes memory result = Callee(address(new Unaligned())).bytesOf(1);
        length = result.length;
        first = uint8(result[0]);
    }

    function directTruncated() external returns (bytes memory) {
        return Callee(address(new Truncated())).bytesOf(1);
    }
}
