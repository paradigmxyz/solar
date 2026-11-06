//@ codegen-matrix: standard
//@ run-call: pushWide() => 11, 22, 0, 0xdead
//@ run-call: pushPacked() => 11, 22, 0, 0xdead
// Pushing a storage value into a storage array copies element-wise, so the
// source has to be loaded at its own type. Loading it as the destination
// element type read the slot after a shorter source, which appended the
// adjacent `guard` state variable instead of a zero, and decoded a packed
// narrow source at the destination's width, which appended both of its
// elements in one word.
contract C {
    uint256[3][] wide;
    uint256[3][] packed;
    uint256[2] wideSource;
    uint8[2] packedSource;
    uint256 guard = 0xdead;

    function pushWide() public returns (uint256, uint256, uint256, uint256) {
        wideSource[0] = 11;
        wideSource[1] = 22;
        wide.push(wideSource);
        return (wide[0][0], wide[0][1], wide[0][2], guard);
    }

    function pushPacked() public returns (uint256, uint256, uint256, uint256) {
        packedSource[0] = 11;
        packedSource[1] = 22;
        packed.push(packedSource);
        return (packed[0][0], packed[0][1], packed[0][2], guard);
    }
}
