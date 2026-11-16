//@ codegen-matrix: standard
//@ compile-flags: --allow=9207 --allow=5878
//@ run-call: pick(bool,bytes,bytes) true, 0x616263, 0x => 0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
//@ run-call: pick(bool,bytes,bytes) false, 0x616263, 0x => 0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//@ run-call: chain(uint256) 3 => 0x12771355e46cd47c71ed1721fd5319b383cca3a1f9fce3aa1c8cd3bd37af20d7
//@ run-call: chain(uint256) 0 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: signedBy(bool) true => true
//@ run-call: signedBy(bool) false => false
//@ run-call: sendOrSkip(bool) false => false
//@ run-call: sendOrSkip(bool) true => false
//@ run-call: transferTwice() =>

// A call that branches on its own failure still works in the middle of an expression: a
// conditional, a loop, and the right side of a short-circuit.
contract PrecompileControlFlow {
    address constant SIGNER = 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf;

    function pick(bool first, bytes memory a, bytes memory b) external pure returns (bytes32) {
        return first ? sha256(a) : sha256(b);
    }

    function chain(uint256 n) external pure returns (bytes32 h) {
        for (uint256 i; i < n; ++i) h = sha256(abi.encodePacked(h));
    }

    function signedBy(bool check) external pure returns (bool) {
        return check
            && ecrecover(
                bytes32(uint256(1)),
                28,
                0x6673ffad2147741f04772b6f921f0ba6af0c1e77fc439e65c36dedf4092e8898,
                0x4c1a971652e0ada880120ef8025e709fff2080c4a39aae068d12eed009b68c89
            ) == SIGNER;
    }

    function sendOrSkip(bool go) external returns (bool ok) {
        ok = go && payable(address(0xbeef)).send(1);
    }

    function transferTwice() external {
        payable(address(0xbeef)).transfer(0);
        payable(address(0xbeef)).transfer(0);
    }
}
