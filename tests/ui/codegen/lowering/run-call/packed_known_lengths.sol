//@ codegen-matrix: standard native
//@[mir] compile-flags: --evm-version paris
//@[gas] compile-flags: --evm-version paris
//@[size] compile-flags: --evm-version paris
//@[native] compile-flags: -Ogas --evm-version cancun
//@ run-call: concat 0x => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223
//@ run-call: concat 0xfedc => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223fedc
//@ run-call: cut 0 => 0xaabb
//@ run-call: cut 32 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20aabb

contract PackedKnownLengths {
    function concat(bytes memory suffix) external pure returns (bytes memory) {
        bytes memory first = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        bytes memory second = hex"2223";
        return bytes.concat(first, second, suffix);
    }

    function cut(uint256 n) external pure returns (bytes memory) {
        require(n <= 33);
        bytes memory first = hex"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021";
        assembly {
            mstore(first, n)
        }
        return bytes.concat(first, hex"aabb");
    }
}
