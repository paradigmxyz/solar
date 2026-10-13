//@ codegen-matrix: standard
//@ run-call: pick true => 0xabab
//@ run-call: pick false => 0xcdcd
//@ run-call: packed true => 0xffabab
//@ run-call: packed false => 0xffcdcd
//@ run-call: pairHash true => 0x98a8c5b885b4afb94ec25c22d9a58c56845adf11fe8425acda13e43551ce31dd
//@ run-call: pairHash false => 0x1e1c23055d28fc82a073b37eb3e1459832364c57ad9cdff9edd5764b0b4ed2e0
//@ run-call: tuple true => 0xabab, 1
//@ run-call: tuple false => 0xcdcd, 2
//@ run-call: discarded true => 1
//@ run-call: discarded false => 2

// Two hex literals that are not valid UTF-8 have no common type until both
// take their mobile type, `string memory`.
contract TernaryHexLiterals {
    function pick(bool c) external pure returns (bytes memory) {
        return bytes(c ? hex"abab" : hex"cdcd");
    }

    function packed(bool c) external pure returns (bytes memory) {
        return abi.encodePacked(hex"ff", c ? hex"abab" : hex"cdcd");
    }

    function pairHash(bool c) external pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                hex"ff",
                c
                    ? hex"96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f"
                    : hex"863fcfbbee75e679d2a43818f377afc8a63590091c47f069b36b7cf8a4d4cad2"
            )
        );
    }

    function tuple(bool c) external pure returns (bytes memory, uint256) {
        (string memory s, uint256 n) = c ? (hex"abab", 1) : (hex"cdcd", 2);
        return (bytes(s), n);
    }

    function discarded(bool c) external pure returns (uint256 n) {
        c ? hex"abab" : hex"cdcd";
        n = c ? 1 : 2;
    }
}
