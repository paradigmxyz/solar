//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: invoke 0x1234 => 2, 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
//@[gas] run-call: invoke 0x1234 => 2, 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
//@[size] run-call: invoke 0x1234 => 2, 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
//@[none] run-call: invoke 0x12345678 => 4, 0x30ca65d5da355227c97ff836c9c6719af9d3835fc6bc72bddc50eeecc1bb2b25
//@[gas] run-call: invoke 0x12345678 => 4, 0x30ca65d5da355227c97ff836c9c6719af9d3835fc6bc72bddc50eeecc1bb2b25
//@[size] run-call: invoke 0x12345678 => 4, 0x30ca65d5da355227c97ff836c9c6719af9d3835fc6bc72bddc50eeecc1bb2b25

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
