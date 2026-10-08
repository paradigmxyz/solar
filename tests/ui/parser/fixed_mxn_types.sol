// `[u]fixedMxN` names with a valid size are elementary types.
contract C {
    fixed40x40 storeMe;
    mapping(uint256 => fixed80x80) m9;
    mapping(address => ufixed16x10) m11;
    mapping(fixed256x0 => ufixed8x80[]) m12;

    function f(ufixed x, fixed32x32 y) public pure {
        ufixed8x8 a;
        fixed b;
        fixed128x18[] memory c;
    }

    // Other names stay identifiers.
    uint fixed0x0 = 0;
    uint fixed7x1 = 0;
    uint fixed08x8 = 0;
    uint fixed8x08 = 0;
    uint fixed8x81 = 0;
    uint fixed264x1 = 0;
    uint ufixed8x = 0;
    uint fixedx8 = 0;
    uint fixed8x8x8 = 0;
}
