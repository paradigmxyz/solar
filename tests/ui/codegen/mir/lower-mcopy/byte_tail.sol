//@ codegen-matrix: standard
//@ compile-flags: --evm-version paris
//@ run-call: single 0x112233, 0xa7 => 0x112233a7
//@ run-call: single 0x, 0xb8 => 0xb8
//@ run-call: wordAndByte 0xccdd, 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f, 0xa7 => 0xccdd000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1fa7

contract ByteTail {
    function single(bytes memory prefix, uint8 value) external pure returns (bytes memory) {
        bytes memory source = new bytes(1);
        source[0] = bytes1(value);
        return bytes.concat(prefix, source);
    }

    function wordAndByte(bytes memory prefix, bytes32 head, uint8 tail)
        external
        pure
        returns (bytes memory)
    {
        bytes memory source = new bytes(33);
        assembly {
            mstore(add(source, 32), head)
            mstore8(add(source, 64), tail)
        }
        return bytes.concat(prefix, source);
    }
}
