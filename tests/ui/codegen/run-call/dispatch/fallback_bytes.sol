//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: invoke 0x => 0, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: invoke 0x1234 => 2, 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
//@ run-call: invoke 0x12345678 => 4, 0x30ca65d5da355227c97ff836c9c6719af9d3835fc6bc72bddc50eeecc1bb2b25

contract FallbackBytes {
    function invoke(bytes memory input) external returns (uint256, bytes32) {
        (bool success, bytes memory result) = address(this).call(input);
        require(success);
        return (result.length, keccak256(result));
    }

    fallback(bytes calldata input) external returns (bytes memory) {
        return input;
    }
}
