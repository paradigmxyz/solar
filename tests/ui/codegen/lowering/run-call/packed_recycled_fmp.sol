//@ codegen-matrix: standard native native_size
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@[native_size] compile-flags: -Osize --evm-version cancun
//@ run-call: payload => 0xabcd22
//@ run-call: length => 0x112255
//@ run-call: overlap => 33

contract PackedRecycledFmp {
    function payload() external pure returns (bytes memory) {
        bytes memory old = new bytes(32);
        assembly {
            mstore(0x40, old)
        }
        bytes memory value = hex"1111";
        assembly {
            mstore(add(old, 32), shl(240, 0xabcd))
        }
        return bytes.concat(value, hex"22");
    }

    function length() external pure returns (bytes memory) {
        bytes memory old = new bytes(32);
        assembly {
            mstore(0x40, old)
        }
        bytes memory value = hex"11223344";
        assembly {
            mstore(old, 2)
        }
        return bytes.concat(value, hex"55");
    }

    function overlap() external pure returns (uint256) {
        bytes memory value = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        assembly {
            mstore(0x40, add(value, 32))
        }
        bytes memory copy = bytes.concat(value, "");
        return uint8(copy[32]);
    }
}
