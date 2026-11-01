//@ codegen-matrix: standard
//@ run-call: sum 4 => 10
//@ run-call: pattern => 0x0101010101010101010101010101010101010101010101010101010101010101
//@ run-call: greet "sol" => "hello, sol"
//@ run-call-fail: share 0 => Panic(18)
//@ run-call: share 3 => 33

// A contract tagged `@custom:solar-optimize size` is compiled for size in every optimized build,
// including the gas revision here, and computes the same results.
/// @custom:solar-optimize size
contract Small {
    function sum(uint256 n) external pure returns (uint256 total) {
        for (uint256 i = 1; i <= n; i++) {
            total += i;
        }
    }

    function pattern() external pure returns (bytes32) {
        return 0x0101010101010101010101010101010101010101010101010101010101010101;
    }

    function greet(string calldata name) external pure returns (string memory) {
        return string.concat("hello, ", name);
    }

    function share(uint256 parts) external pure returns (uint256) {
        return 100 / parts;
    }
}
