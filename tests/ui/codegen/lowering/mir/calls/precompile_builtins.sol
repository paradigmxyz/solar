//@[mir] filecheck:
// CHECK: @module
// CHECK: staticcall
// A digest lands in scratch memory, and a failed call forwards its return data.
// CHECK-LABEL: fn @shaWord(
// CHECK: [[SUCCESS:v[0-9]+]] = staticcall {{v[0-9]+}}, 2, {{v[0-9]+}}, 32, 0, 32
// CHECK-NEXT: jumpi [[SUCCESS]], [[DONE:bb[0-9]+]], [[FAILED:bb[0-9]+]]
// CHECK: [[FAILED]] [cold]:
// CHECK: returndatacopy 0, 0
// CHECK: [[DONE]]:
// CHECK-NEXT: mload 0
// CHECK-LABEL: fn @ripemdWord(
// CHECK: [[SUCCESS:v[0-9]+]] = staticcall {{v[0-9]+}}, 3, {{v[0-9]+}}, 32, 0, 32
// CHECK-NEXT: jumpi [[SUCCESS]], [[DONE:bb[0-9]+]], [[FAILED:bb[0-9]+]]
//@ revisions: homestead byzantium constantinople osaka mir
//@[homestead] compile-flags: --evm-version homestead --emit=abi,bin
//@[byzantium] compile-flags: --evm-version byzantium --emit=abi,bin
//@[constantinople] compile-flags: --evm-version constantinople --emit=abi,bin
//@[osaka] compile-flags: --evm-version osaka --emit=abi,bin
//@[mir] compile-flags: --evm-version osaka -Zdump=mir
//@ run-call: recover => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: recoverInvalid => 0x0000000000000000000000000000000000000000
//@ run-call: sha => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@ run-call: ripemd => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@ run-call: shaLiteral => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@ run-call: ripemdLiteral => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc
//@ run-call: shaEmpty => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@ run-call: ripemdEmpty => 0x9c1185a5c5e9fc54612808977ee8f548b2258d31
//@ run-call: shaWord 1 => 0xec4916dd28fc4c10d78e287ca5d9cc51ee1ae73cbfde08c6b37324cbfaac8bc5
//@ run-call: ripemdWord 1 => 0xae387fcfeb723c3f5964509af111cf5a67f30661

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

    function shaWord(uint256 x) external pure returns (bytes32) {
        return sha256(abi.encodePacked(x));
    }

    function ripemdWord(uint256 x) external pure returns (bytes20) {
        return ripemd160(abi.encodePacked(x));
    }
}
