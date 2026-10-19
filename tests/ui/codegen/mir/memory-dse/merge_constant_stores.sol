//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: prefix() => "0x"
//@ run-call: hexPrefix 0x4241 => "0xAB"
//@ run-call: unknownWindow 0x1111111111111111111111111111111111111111111111111111111111111111 => 0x3078111111111111111111111111111111111111111111111111111111111111
//@ run-call: readBetween() => 0x3078000000000000000000000000000000000000000000000000000000000000, 0x3000000000000000000000000000000000000000000000000000000000000000
//@ run-call: overwriteByte() => 0x0203000000000000000000000000000000000000000000000000000000000000
//@ run-call: straddle() => 0x0000000000000000000000000000000030780000000000000000000000000000, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: gap() => 0x3000000000000000000000000000000000000000000000000000000000000000, 0x7800000000000000000000000000000000000000000000000000000000000000

contract MergeConstantStores {
    // The two byte stores into the zeroed allocation become one word store.
    // CHECK-LABEL: fn @prefix
    // CHECK: mstore {{.*}}, 0x3078000000000000000000000000000000000000000000000000000000000000
    // CHECK-NOT: mstore8
    function prefix() external pure returns (string memory) {
        bytes memory buffer = new bytes(2);
        buffer[0] = "0";
        buffer[1] = "x";
        return string(buffer);
    }

    // Only the constant prefix merges; the bytes of `v` stay byte stores.
    // CHECK-LABEL: fn @hexPrefix
    // CHECK: mstore {{.*}}, 0x3078000000000000000000000000000000000000000000000000000000000000
    // CHECK: mstore8
    // CHECK: mstore8
    function hexPrefix(uint256 v) external pure returns (string memory) {
        bytes memory buffer = new bytes(4);
        buffer[0] = "0";
        buffer[1] = "x";
        buffer[2] = bytes1(uint8(v));
        buffer[3] = bytes1(uint8(v >> 8));
        return string(buffer);
    }

    // The word under the stores is unknown, so no window is known.
    // CHECK-LABEL: fn @unknownWindow
    // CHECK: mstore8 128, 48
    // CHECK: mstore8 129, 120
    function unknownWindow(bytes32 x) external pure returns (bytes32) {
        assembly {
            mstore(0x80, x)
            mstore8(0x80, 0x30)
            mstore8(0x81, 0x78)
            return(0x80, 32)
        }
    }

    // The load observes the memory between the two byte stores.
    // CHECK-LABEL: fn @readBetween
    // CHECK: mstore8 128, 48
    // CHECK: mload 128
    // CHECK: mstore8 129, 120
    function readBetween() external pure returns (bytes32, bytes32) {
        assembly {
            mstore(0x80, 0)
            mstore8(0x80, 0x30)
            mstore(0xa0, mload(0x80))
            mstore8(0x81, 0x78)
            return(0x80, 64)
        }
    }

    // The last store of a byte wins, as in program order.
    // CHECK-LABEL: fn @overwriteByte
    // CHECK-NOT: mstore8
    // CHECK: mstore 128, 0x203000000000000000000000000000000000000000000000000000000000000
    function overwriteByte() external pure returns (bytes32) {
        assembly {
            mstore(0x80, 0)
            mstore8(0x80, 1)
            mstore8(0x80, 2)
            mstore8(0x81, 3)
            return(0x80, 32)
        }
    }

    // The window starts at 0x90 and takes its bytes from two known words.
    // CHECK-LABEL: fn @straddle
    // CHECK-NOT: mstore8
    // CHECK: mstore 144, 0x3078000000000000000000000000000000000000000000000000000000000000
    function straddle() external pure returns (bytes32, bytes32) {
        assembly {
            mstore(0x80, 0)
            mstore(0xa0, 0)
            mstore8(0x90, 0x30)
            mstore8(0x91, 0x78)
            return(0x80, 64)
        }
    }

    // The stores are 32 bytes apart, so they share no window.
    // CHECK-LABEL: fn @gap
    // CHECK: mstore8 128, 48
    // CHECK: mstore8 160, 120
    function gap() external pure returns (bytes32, bytes32) {
        assembly {
            mstore(0x80, 0)
            mstore(0xa0, 0)
            mstore8(0x80, 0x30)
            mstore8(0xa0, 0x78)
            return(0x80, 64)
        }
    }
}
