//@ codegen-matrix: standard
//@[gas] run-call: run 1, 2, 3, 4, 5 => 751
//@[gas] run-call: run 0, 0, 0, 0, 0 => 345
//@[gas] run-call: run 7, 100, 1000, 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 21794
//~[mir]? ERROR: codegen cannot preserve values across a low-memory write in `f0`
//~[mir,size]? ERROR: codegen cannot preserve values across a low-memory write in `f1`

contract ResidentArgDupReachResetFmp {
    uint256 s;
    function f0(uint256 p0, uint256 p1, uint256 p2, uint256 p3, uint256 p4, uint256 p5, uint256 p6, uint256 p7, uint256 p8, uint256 p9, uint256 p10, uint256 p11) internal returns (uint256) {
        unchecked {
            uint256 l1 = p4 & 42;
            uint256 l3 = p5 | p6;
            uint256 l4 = l3 & 20;
            uint256 l5 = l3 + p11;
            uint256 l9 = p6 + l3;
            uint256 l10 = p4 ^ l9;
            uint256 l11 = p1 + 9;
            uint256 l12 = l11 - 37;
            uint256 l13 = p7 ^ l1;
            s += p0;
            return p1 + l5 + l13 + p5 + l4 + l10 + p4 + p10 + l3 + p7 + l12;
        }
    }
    function f1(uint256 p0, uint256 p1, uint256 p2, uint256 p3, uint256 p4) internal returns (uint256) {
        unchecked {
            uint256 l0 = f0(p3, p2, p1, p0, p0, p4, p1, p1, p4, p4, p3, p1);
            uint256 l1 = p3 | p0;
            bytes memory bl2 = new bytes(32);
            uint256 l2 = bl2.length + p4;
            uint256 l3 = f0(p4, p2, p2, l1, p1, l1, l2, l0, l1, p0, p4, l1);
            uint256 l4 = p0 | 16;
            uint256 l5 = l4;
            bytes memory bl7 = new bytes(32);
            uint256 l7 = bl7.length + p3;
            assembly { mstore(0x40, 0x80) }
            bytes memory bl9 = new bytes(32);
            uint256 l9 = bl9.length + p2;
            uint256 l10 = l7 ^ 31;
            uint256 l11 = l2 - 35;
            return f0(l2, l11, p3, p0, p2, p4, l5, l3, l0, l2, l9, l2) + p4 + p2 + l2 + p3 + l9 + l1 + l4 + l10 + p0 + p1 + l0 + l7;
        }
    }
    function run(uint256 a0, uint256 a1, uint256 a2, uint256 a3, uint256 a4) external returns (uint256) {
        return f1(a0, a1, a2, a3, a4);
    }
}
