//@ revisions: homestead byzantium constantinople osaka
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
//@[constantinople] compile-flags: --evm-version constantinople
//@[osaka] compile-flags: --evm-version osaka
//@ run-call: recover() => 0x7e5f4552091a69125d5dfcb7b8c2659029395bdf
//@ run-call: recoverInvalid() => 0x0000000000000000000000000000000000000000
//@ run-call: sha() => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@ run-call: ripemd() => 0x8eb208f7e05d987a9b044a8e98c6b087f15a0bfc

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
}
