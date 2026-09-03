contract SignedArith {
    function divMinNeg1(int256 a, int256 b) external pure returns (int256) { return a / b; }
    function divMinNeg1U(int256 a, int256 b) external pure returns (int256) { unchecked { return a / b; } }
    function modNeg(int256 a, int256 b) external pure returns (int256) { return a % b; }
    function modNegU(int256 a, int256 b) external pure returns (int256) { unchecked { return a % b; } }
    function neg(int256 a) external pure returns (int256) { return -a; }
    function negU(int256 a) external pure returns (int256) { unchecked { return -a; } }
    function neg8(int8 a) external pure returns (int8) { return -a; }
    function neg8U(int8 a) external pure returns (int8) { unchecked { return -a; } }
    function div8(int8 a, int8 b) external pure returns (int8) { return a / b; }
    function div8U(int8 a, int8 b) external pure returns (int8) { unchecked { return a / b; } }
    function mod8(int8 a, int8 b) external pure returns (int8) { return a % b; }
    function mul8(int8 a, int8 b) external pure returns (int8) { return a * b; }
    function mul8U(int8 a, int8 b) external pure returns (int8) { unchecked { return a * b; } }
    function add8(int8 a, int8 b) external pure returns (int8) { return a + b; }
    function sub8(int8 a, int8 b) external pure returns (int8) { return a - b; }
    function mul128(int128 a, int128 b) external pure returns (int128) { return a * b; }
    function mul256(int256 a, int256 b) external pure returns (int256) { return a * b; }
    function shr(int256 a, uint256 s) external pure returns (int256) { return a >> s; }
    function shl(int256 a, uint256 s) external pure returns (int256) { return a << s; }
    function shr8(int8 a, uint8 s) external pure returns (int8) { return a >> s; }
    function shl8(int8 a, uint8 s) external pure returns (int8) { return a << s; }
    function shl8U(int8 a, uint8 s) external pure returns (int8) { unchecked { return a << s; } }
    function powS(int256 a, uint256 e) external pure returns (int256) { return a ** e; }
    function powSU(int256 a, uint256 e) external pure returns (int256) { unchecked { return a ** e; } }
    function pow8(int8 a, uint8 e) external pure returns (int8) { return a ** e; }
    function powNeg1(uint256 e) external pure returns (int256) { int256 b = -1; return b ** e; }
    function powNeg2(uint256 e) external pure returns (int256) { int256 b = -2; return b ** e; }
    function powMin(uint256 e) external pure returns (int256) { int256 b = type(int256).min; return b ** e; }
    function powBase2(uint256 e) external pure returns (int8) { int8 b = 2; return b ** e; }
    function powLit(int256 a) external pure returns (int256) { return a ** 2; }
    function powLit3(int256 a) external pure returns (int256) { return a ** 3; }
    function powLit3_8(int8 a) external pure returns (int8) { return a ** 3; }
    function powLitU(int256 a) external pure returns (int256) { unchecked { return a ** 3; } }
    function trunc(int256 a) external pure returns (int8) { return int8(a); }
    function truncWide(int256 a) external pure returns (int128) { return int128(a); }
    function widen(int8 a) external pure returns (int256) { return a; }
    function toUint(int256 a) external pure returns (uint256) { return uint256(a); }
    function toInt(uint256 a) external pure returns (int256) { return int256(a); }
    function toUint8(int8 a) external pure returns (uint8) { return uint8(a); }
    function toInt8(uint8 a) external pure returns (int8) { return int8(a); }
    function cmp(int8 a, int8 b) external pure returns (bool, bool, bool, bool) { return (a < b, a <= b, a > b, a >= b); }
    function cmpMixed(int8 a, int256 b) external pure returns (bool) { return a < b; }
    function bitAnd(int8 a, int8 b) external pure returns (int8) { return a & b; }
    function bitOr(int8 a, int8 b) external pure returns (int8) { return a | b; }
    function bitXor(int8 a, int8 b) external pure returns (int8) { return a ^ b; }
    function bitNot(int8 a) external pure returns (int8) { return ~a; }
    function bitNotWiden(int8 a) external pure returns (int256) { return ~a; }
    function abs(int256 a) external pure returns (int256) { return a < 0 ? -a : a; }
    function absU(int256 a) external pure returns (uint256) { unchecked { return a < 0 ? uint256(-a) : uint256(a); } }
    function incMax(int8 a) external pure returns (int8) { a++; return a; }
    function decMin(int8 a) external pure returns (int8) { return --a; }
    function incU(int8 a) external pure returns (int8, int8) { unchecked { int8 b = a++; return (a, b); } }
    function minDivLit(int256 a) external pure returns (int256) { return a / -1; }
    function minDivLitU(int256 a) external pure returns (int256) { unchecked { return a / -1; } }
    function litDivMin(int256 a) external pure returns (int256) { return -1 / a; }
    function modLit(int256 a) external pure returns (int256) { return a % -1; }
    function divByZero(int256 a) external pure returns (int256) { int256 z = 0; return a / z; }
    function modByZero(int8 a) external pure returns (int8) { int8 z = 0; return a % z; }
    function shrBig(int256 a) external pure returns (int256) { return a >> 256; }
    function shlBig(int256 a) external pure returns (int256) { return a << 256; }
    function shr255(int256 a) external pure returns (int256) { return a >> 255; }
    function shrLit(uint256 s) external pure returns (int256) { return -1 >> s; }
    function shlLit(uint256 s) external pure returns (int256) { return -1 << s; }
    function shrLit8(uint256 s) external pure returns (int8) { int8 a = -128; return a >> s; }
    function shlLit8(uint256 s) external pure returns (int8) { int8 a = -1; return a << s; }
    function ternaryLit(bool c) external pure returns (int8) { return c ? int8(-128) : int8(127); }
    function ternaryMixed(bool c, int8 a) external pure returns (int256) { return c ? a : -1; }
    function mulDivRound(int256 a) external pure returns (int256) { return a / 3 * 3 + a % 3; }
    function minMod(int256 a, int256 b) external pure returns (int256) { unchecked { return (a % b) * (b < 0 ? -1 : int256(1)); } }
    function subSigned(uint256 a, uint256 b) external pure returns (int256) { return int256(a) - int256(b); }
    function compareUintInt(uint8 a, int16 b) external pure returns (bool) { return int16(uint16(a)) > b; }
    function sumSigned(int8[3] calldata a) external pure returns (int256 s) { for (uint256 i; i < 3; i++) s += a[i]; }
    function sumSigned8(int8[3] calldata a) external pure returns (int8 s) { for (uint256 i; i < 3; i++) s += a[i]; }
    function fromBool(bool b) external pure returns (int8) { return b ? int8(-1) : int8(0); }
    function mulmodS(uint256 a, uint256 b, uint256 m) external pure returns (uint256) { return mulmod(a, b, m); }
    function addmodS(uint256 a, uint256 b, uint256 m) external pure returns (uint256) { return addmod(a, b, m); }
    function mulmodZero(uint256 a, uint256 b) external pure returns (uint256) { uint256 z = 0; return mulmod(a, b, z); }
}
