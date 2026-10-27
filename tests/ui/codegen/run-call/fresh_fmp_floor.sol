//@ codegen-matrix: standard
//@ run-call: spills 0 => 78338746147236970124700731725183845421594913511827187288591969170390706511537
//@ run-call: spills 1 => 56844104204168017825590069259861998737668126311882522109025188529445603644917
//@ run-call: spills 7 => 27797557954442776764947608843914043294405182275560171483200161595284771394521
//@ run-call: spills 100 => 104105515180805866949124345148722461624639736772847398061203472501805378783670
//@ run-call: frames 0 => 38679465208037746178823362447866013171712401150649964742780979969090416962160
//@ run-call: frames 1 => 91696820555953731586845425027559412493408880143556788743236580825721320747226
//@ run-call: frames 7 => 45440633778322076264030570702330852782991024588218507816095131621076023727763
//@ run-call: frames 100 => 4831998004491699108763654109047991634616664475409784461218397081769887605733
// Every read of the free memory pointer is the first of its entry, so the entry reads the heap
// floor and skips initializing the slot. The floor must clear the entry's spill slots and the
// frames of the internal functions it calls, which stay live while the buffer is in use.
contract Test {
    uint256 s0 = 3;
    uint256 s1 = 5;
    uint256 s2 = 7;
    uint256 s3 = 11;
    uint256 s4 = 13;
    uint256 s5 = 17;
    uint256 s6 = 19;
    uint256 s7 = 23;
    uint256 s8 = 29;
    uint256 s9 = 31;
    uint256 s10 = 37;
    uint256 s11 = 41;
    uint256 s12 = 43;
    uint256 s13 = 47;
    uint256 s14 = 53;
    uint256 s15 = 59;
    uint256 s16 = 61;
    uint256 s17 = 67;

    function spills(uint256 n) external view returns (uint256) {
        uint256 t0 = s0 * 2 + n;
        uint256 t1 = s1 * 3 + n;
        uint256 t2 = s2 * 5 + n;
        uint256 t3 = s3 * 7 + n;
        uint256 t4 = s4 * 11 + n;
        uint256 t5 = s5 * 13 + n;
        uint256 t6 = s6 * 17 + n;
        uint256 t7 = s7 * 19 + n;
        uint256 t8 = s8 * 23 + n;
        uint256 t9 = s9 * 29 + n;
        uint256 t10 = s10 * 31 + n;
        uint256 t11 = s11 * 37 + n;
        uint256 t12 = s12 * 41 + n;
        uint256 t13 = s13 * 43 + n;
        uint256 t14 = s14 * 47 + n;
        uint256 t15 = s15 * 53 + n;
        uint256 t16 = s16 * 59 + n;
        uint256 t17 = s17 * 61 + n;
        bytes memory buf = new bytes(n % 5 + 64);
        buf[1] = bytes1(uint8(n));
        uint256 h = uint256(keccak256(buf));
        unchecked {
            return h + t0 + t1 * 2 + t2 * 3 + t3 * 4 + t4 * 5 + t5 * 6 + t6 * 7 + t7 * 8 + t8 * 9
                + t9 * 10 + t10 * 11 + t11 * 12 + t12 * 13 + t13 * 14 + t14 * 15 + t15 * 16
                + t16 * 17 + t17 * 18;
        }
    }

    function mix(uint256 n) internal view returns (uint256 acc) {
        uint256 t0 = s0 * 2 + n;
        uint256 t1 = s1 * 3 + n;
        uint256 t2 = s2 * 5 + n;
        uint256 t3 = s3 * 7 + n;
        uint256 t4 = s4 * 11 + n;
        uint256 t5 = s5 * 13 + n;
        uint256 t6 = s6 * 17 + n;
        uint256 t7 = s7 * 19 + n;
        uint256 t8 = s8 * 23 + n;
        uint256 t9 = s9 * 29 + n;
        uint256 t10 = s10 * 31 + n;
        uint256 t11 = s11 * 37 + n;
        uint256 t12 = s12 * 41 + n;
        uint256 t13 = s13 * 43 + n;
        uint256 t14 = s14 * 47 + n;
        uint256 t15 = s15 * 53 + n;
        uint256 t16 = s16 * 59 + n;
        uint256 t17 = s17 * 61 + n;
        for (uint256 i; i < n % 3 + 1; ++i) {
            acc += t0 ^ (t17 + i);
            acc += t1 * 2 + t2 * 3 + t3 * 4 + t4 * 5 + t5 * 6 + t6 * 7 + t7 * 8 + t8 * 9;
            acc += t9 * 10 + t10 * 11 + t11 * 12 + t12 * 13 + t13 * 14 + t14 * 15 + t15 * 16
                + t16 * 17;
        }
    }

    function frames(uint256 n) external view returns (uint256) {
        bytes memory buf = new bytes(n % 5 + 64);
        buf[1] = bytes1(uint8(n));
        buf[40] = bytes1(uint8(n + 1));
        uint256 first = mix(n);
        uint256 second = mix(n + 1);
        unchecked {
            return uint256(keccak256(buf)) + first + second;
        }
    }
}
