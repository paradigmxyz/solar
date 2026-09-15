//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@ run-call: probe 0 => 0, 221, 51
//@ run-call: probe 34 => 221, 221, 51
//@ run-call: probe 255 => 255, 221, 51
//@ run-call: doubleProbe => 34

// Internal returns leave this activation. Spill liveness must not invent a return edge to the
// loop preheader and retain stores whose only loads precede the loop. Both branch paths retain
// the loaded word until its store; a second internal call exercises the caller continuation.
contract ReturnCopy {
    function probe(uint256 seed) external pure returns (uint256, uint256, uint256) {
        bytes memory input = new bytes(3);
        input[0] = bytes1(uint8(seed));
        input[1] = 0x22;
        input[2] = 0x33;
        bytes memory output = copy(input, 0x22);
        return (uint8(output[0]), uint8(output[1]), uint8(output[2]));
    }
    function doubleProbe() external pure returns (uint256) {
        return uint8(copy(copy(hex"222233", 0x22), 0xdd)[1]);
    }

    function run(bytes memory input, uint256 marker) public pure returns (bytes memory) {
        return copy(input, marker);
    }
    function twice(bytes memory input, uint256 marker) public pure returns (bytes memory) {
        return copy(copy(input, marker), marker);
    }
    function copy(bytes memory input, uint256 marker) internal pure returns (bytes memory out) {
        assembly {
            out := mload(0x40)
            let p := add(input, 32)
            let end := add(p, mload(input))
            let delta := sub(out, input)
            for {} lt(p, end) {} {
                let word := mload(p)
                if eq(byte(0, word), marker) {
                    mstore(add(p, delta), not(word))
                    p := add(p, 1)
                    if iszero(lt(p, end)) { break }
                    continue
                }
                mstore(add(p, delta), word)
                p := add(p, 1)
            }
            let n := sub(p, add(input,32))
            mstore(out, n)
            mstore(add(add(out,32),n), 0)
            mstore(0x40, add(add(out,64),n))
        }
    }
}
