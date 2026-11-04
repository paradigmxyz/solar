//@ codegen-matrix: standard
//@ run-call: C::noop => 0x08c379a000000000000000000000000000000000000000000000000000000000
//@ run-call: C::shifted => 32
//@ run-call: C::short => 0xaabbcc0000000000000000000000000000000000000000000000000000000000
//@ run-call: C::weirollMessage => 0x496e73756666696369656e74206f757470757400000000000000000000000000

// A memory `bytes` variable assigned in inline assembly still converts to `bytesN` by
// reading its data, not by converting the pointer. Weiroll's VM decodes revert reasons this
// way, shifting the pointer over the selector and the offset word.

contract C {
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
}
