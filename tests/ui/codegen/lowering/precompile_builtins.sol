//@ filecheck:
// CHECK: @module
//@ revisions: homestead byzantium constantinople osaka mir
//@[homestead] compile-flags: --evm-version homestead --emit=abi,bin
//@[byzantium] compile-flags: --evm-version byzantium --emit=abi,bin
//@[constantinople] compile-flags: --evm-version constantinople --emit=abi,bin
//@[osaka] compile-flags: --evm-version osaka --emit=abi,bin
//@[mir] compile-flags: --evm-version osaka -Zdump=mir
//@[homestead] normalize-stdout-test: "(?s).+" -> ""
//@[byzantium] normalize-stdout-test: "(?s).+" -> ""
//@[constantinople] normalize-stdout-test: "(?s).+" -> ""
//@[osaka] normalize-stdout-test: "(?s).+" -> ""
//@[homestead] run-call: recover() => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@[byzantium] run-call: recover() => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@[constantinople] run-call: recover() => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@[osaka] run-call: recover() => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@[homestead] run-call: recoverInvalid() => 0x0000000000000000000000000000000000000000
//@[byzantium] run-call: recoverInvalid() => 0x0000000000000000000000000000000000000000
//@[constantinople] run-call: recoverInvalid() => 0x0000000000000000000000000000000000000000
//@[osaka] run-call: recoverInvalid() => 0x0000000000000000000000000000000000000000
//@[homestead] run-call: sha() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[byzantium] run-call: sha() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[constantinople] run-call: sha() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[osaka] run-call: sha() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[homestead] run-call: ripemd() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[byzantium] run-call: ripemd() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[constantinople] run-call: ripemd() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[osaka] run-call: ripemd() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[homestead] run-call: shaLiteral() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[byzantium] run-call: shaLiteral() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[constantinople] run-call: shaLiteral() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[osaka] run-call: shaLiteral() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@[homestead] run-call: ripemdLiteral() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[byzantium] run-call: ripemdLiteral() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[constantinople] run-call: ripemdLiteral() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[osaka] run-call: ripemdLiteral() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@[homestead] run-call: shaEmpty() => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@[byzantium] run-call: shaEmpty() => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@[constantinople] run-call: shaEmpty() => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@[osaka] run-call: shaEmpty() => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@[homestead] run-call: ripemdEmpty() => 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@[byzantium] run-call: ripemdEmpty() => 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@[constantinople] run-call: ripemdEmpty() => 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@[osaka] run-call: ripemdEmpty() => 0x9c1185a5c5e9fc54612808977ee8f548b2258d31

contract PrecompileBuiltins {
    function recover() external pure returns (address) {
        return ecrecover(
            bytes32(uint256(1)),
            28,
            0x6673ffad2147741f04772b6f921f0ba6af0c1e77fc439e65c36dedf4092e8898,
            0x4c1a971652e0ada880120ef8025e709fff2080c4a39aae068d12eed009b68c89
        );
    }

    function recoverInvalid() external pure returns (address) {
        return ecrecover(bytes32(0), 0, bytes32(0), bytes32(0));
    }

    function sha() external pure returns (bytes32) {
        return sha256(bytes("abc"));
    }

    function ripemd() external pure returns (bytes20) {
        return ripemd160(bytes("abc"));
    }

    function shaLiteral() external pure returns (bytes32) {
        return sha256("abc");
    }

    function ripemdLiteral() external pure returns (bytes20) {
        return ripemd160("abc");
    }

    function shaEmpty() external pure returns (bytes32) {
        return sha256("");
    }

    function ripemdEmpty() external pure returns (bytes20) {
        return ripemd160("");
    }
}
