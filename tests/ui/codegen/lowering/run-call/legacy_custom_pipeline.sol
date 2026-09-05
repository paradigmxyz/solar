//@ revisions: gas_default gas_none gas_custom size_default size_none size_custom none_custom
//@ compile-flags: --evm-version byzantium
//@[gas_default] compile-flags: -Ogas -Zevm-ir-pipeline=default
//@[gas_none] compile-flags: -Ogas -Zevm-ir-pipeline=none
//@[gas_custom] compile-flags: -Ogas -Zevm-ir-pipeline=peephole
//@[size_default] compile-flags: -Osize -Zevm-ir-pipeline=default
//@[size_none] compile-flags: -Osize -Zevm-ir-pipeline=none
//@[size_custom] compile-flags: -Osize -Zevm-ir-pipeline=peephole
//@[none_custom] compile-flags: -Onone -Zevm-ir-pipeline=peephole
//@ run-call: LegacyTargetPipeline::shift 3, 5 => 96, 0, 0
//@ run-call: LegacyTargetPipeline::shift -3, 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639933, 115792089237316195423570985008687907853269984665640564039457584007913129639933, -3
//@ run-call: LegacyTargetPipeline::shift -3, 1 => 115792089237316195423570985008687907853269984665640564039457584007913129639930, 57896044618658097711785492504343953926634992332820282019728792003956564819966, -2
//@ run-call: LegacyTargetPipeline::shift -3, 255 => 57896044618658097711785492504343953926634992332820282019728792003956564819968, 1, -1
//@ run-call: LegacyTargetPipeline::shift -3, 256 => 0, 0, -1
//@ run-call: LegacyTargetPipeline::shift -3, 300 => 0, 0, -1
//@ run-call: LegacyTargetPipeline::shift -3, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0, 0, -1
//@ run-call: LegacyTargetPipeline::shift -57896044618658097711785492504343953926634992332820282019728792003956564819968, 1 => 0, 28948022309329048855892746252171976963317496166410141009864396001978282409984, -28948022309329048855892746252171976963317496166410141009864396001978282409984
//@ run-call: LegacyTargetPipeline::shift -57896044618658097711785492504343953926634992332820282019728792003956564819968, 255 => 0, 1, -1
//@ run-call: LegacyTargetPipeline::shift 3, 256 => 0, 0, 0
//@ run-call: LegacyTargetPipeline::copy => 0xab01020304050600000000000000000000000000000000000000000000000000
//@ run-call: LegacyDeploymentShift::read; constructor=[-3, 300] => -1
//@ run-call: LegacyDeploymentShift::read; constructor=[-3, 1] => -2

// Required target legalization must run after each user-selected EVM pipeline.
contract LegacyTargetPipeline {
    function shift(int256 value, uint256 amount) external pure
        returns (uint256 left, uint256 right, int256 arithmetic)
    {
        return (uint256(value) << amount, uint256(value) >> amount, value >> amount);
    }

    function copy() external pure returns (bytes32 out) {
        bytes memory source = hex"ab010203040506";
        bytes memory duplicate = abi.decode(abi.encode(source), (bytes));
        assembly { out := mload(add(duplicate, 32)) }
    }
}

contract LegacyDeploymentShift {
    int256 stored;
    constructor(int256 value, uint256 amount) { stored = value >> amount; }
    function read() external view returns (int256) { return stored; }
}
