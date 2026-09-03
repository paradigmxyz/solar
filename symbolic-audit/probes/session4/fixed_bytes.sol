contract FixedBytes {
    function shl(bytes4 b, uint256 s) external pure returns (bytes4) { return b << s; }
    function shr(bytes4 b, uint256 s) external pure returns (bytes4) { return b >> s; }
    function shl32(bytes32 b, uint256 s) external pure returns (bytes32) { return b << s; }
    function shr32(bytes32 b, uint256 s) external pure returns (bytes32) { return b >> s; }
    function shl8(bytes1 b, uint8 s) external pure returns (bytes1) { return b << s; }
    function shrLit(bytes4 b) external pure returns (bytes4) { return b >> 8; }
    function shlLitBig(bytes4 b) external pure returns (bytes4) { return b << 40; }
    function index(bytes4 b, uint256 i) external pure returns (bytes1) { return b[i]; }
    function index32(bytes32 b, uint256 i) external pure returns (bytes1) { return b[i]; }
    function indexLit(bytes4 b) external pure returns (bytes1) { return b[3]; }
    function and(bytes4 a, bytes4 b) external pure returns (bytes4) { return a & b; }
    function or(bytes4 a, bytes4 b) external pure returns (bytes4) { return a | b; }
    function xor(bytes4 a, bytes4 b) external pure returns (bytes4) { return a ^ b; }
    function not(bytes4 a) external pure returns (bytes4) { return ~a; }
    function not1(bytes1 a) external pure returns (bytes1) { return ~a; }
    function eq(bytes4 a, bytes4 b) external pure returns (bool) { return a == b; }
    function lt(bytes4 a, bytes4 b) external pure returns (bool, bool, bool) { return (a < b, a <= b, a > b); }
    function toUint(bytes4 b) external pure returns (uint32) { return uint32(b); }
    function toUintWide(bytes4 b) external pure returns (uint256) { return uint256(uint32(b)); }
    function fromUint(uint32 u) external pure returns (bytes4) { return bytes4(u); }
    function fromUintWide(uint256 u) external pure returns (bytes4) { return bytes4(uint32(u)); }
    function toB32(bytes4 b) external pure returns (bytes32) { return bytes32(b); }
    function toB32u(bytes4 b) external pure returns (uint256) { return uint256(bytes32(b)); }
    function toB2(bytes4 b) external pure returns (bytes2) { return bytes2(b); }
    function toB2u(bytes4 b) external pure returns (uint16) { return uint16(bytes2(b)); }
    function b32ToU(bytes32 b) external pure returns (uint256) { return uint256(b); }
    function uToB32(uint256 u) external pure returns (bytes32) { return bytes32(u); }
    function b20ToAddr(bytes20 b) external pure returns (address) { return address(b); }
    function addrToB20(address a) external pure returns (bytes20) { return bytes20(a); }
    function addrToB32(address a) external pure returns (bytes32) { return bytes32(bytes20(a)); }
    function b1ToU8(bytes1 b) external pure returns (uint8) { return uint8(b); }
    function u8ToB1(uint8 u) external pure returns (bytes1) { return bytes1(u); }
    function concat(bytes4 a, bytes2 b) external pure returns (bytes memory) { return bytes.concat(a, b); }
    function concatDyn(bytes4 a, bytes calldata b) external pure returns (bytes memory) { return bytes.concat(a, b, a); }
    function concatMem(bytes memory a, bytes memory b) external pure returns (bytes memory) { return bytes.concat(a, b); }
    function concatEmpty() external pure returns (bytes memory) { return bytes.concat(); }
    function concatStr(string memory a, string memory b) external pure returns (string memory) { return string.concat(a, b, "!"); }
    function concatStr0(string calldata a) external pure returns (uint256) { return bytes(string.concat(a, "")).length; }
    function b32FromMem(bytes memory m) external pure returns (bytes32 r) { assembly { r := mload(add(m, 32)) } }
    function b32FromDyn(bytes calldata m) external pure returns (bytes32) { return bytes32(m); }
    function b4FromDyn(bytes calldata m) external pure returns (bytes4) { return bytes4(m); }
    function b4FromMem(bytes memory m) external pure returns (bytes4) { return bytes4(m); }
    function b2FromSlice(bytes calldata m) external pure returns (bytes2) { return bytes2(m[1:]); }
    function b4FromSlice(bytes calldata m) external pure returns (bytes4) { return bytes4(m[:3]); }
    function memIndex(bytes memory m, uint256 i) external pure returns (bytes1) { return m[i]; }
    function memIndexWrite(bytes memory m, uint256 i, bytes1 v) external pure returns (bytes memory) { m[i] = v; return m; }
    function cdIndex(bytes calldata m, uint256 i) external pure returns (bytes1) { return m[i]; }
    function strToBytes(string memory s) external pure returns (uint256, bytes1) { bytes memory b = bytes(s); return (b.length, b.length > 0 ? b[0] : bytes1(0)); }
    function bytesToStr(bytes memory b) external pure returns (string memory) { return string(b); }
    function cdStrLen(string calldata s) external pure returns (uint256) { return bytes(s).length; }
    function cdStrIdx(string calldata s, uint256 i) external pure returns (bytes1) { return bytes(s)[i]; }
    function litB4() external pure returns (bytes4) { return "abcd"; }
    function litB4short() external pure returns (bytes4) { return "ab"; }
    function litB4hex() external pure returns (bytes4) { return hex"aabb"; }
    function litB32() external pure returns (bytes32) { return "hello"; }
    function litB1() external pure returns (bytes1) { return 0xff; }
    function litB4num() external pure returns (bytes4) { return 0xdeadbeef; }
    function litB2Lead0() external pure returns (bytes2) { return 0x00ff; }
    function cmpLit(bytes4 a) external pure returns (bool) { return a == "abcd"; }
    function cmpHex(bytes2 a) external pure returns (bool) { return a == hex"ff00"; }
    function orLit(bytes4 a) external pure returns (bytes4) { return a | 0x000000ff; }
    function ternary(bool c, bytes4 a) external pure returns (bytes4) { return c ? a : bytes4(0); }
    function ternaryLit(bool c) external pure returns (bytes4) { return c ? bytes4("ab") : bytes4(0x00000001); }
    function b1Loop(bytes memory m) external pure returns (uint256 s) { for (uint256 i; i < m.length; i++) s += uint8(m[i]); }
    function b32Loop(bytes32 b) external pure returns (uint256 s) { for (uint256 i; i < 32; i++) s += uint8(b[i]); }
    function b4Sig(bytes calldata data) external pure returns (bytes4) { return bytes4(data[:4]); }
    function selectorOf(bytes calldata data) external pure returns (bool) { return bytes4(data) == this.selectorOf.selector; }
    function shift1(bytes1 b) external pure returns (bytes1, bytes1) { return (b >> 4, b << 4); }
    function shiftBig1(bytes1 b) external pure returns (bytes1) { return b >> 8; }
    function shiftBig2(bytes1 b) external pure returns (bytes1) { return b << 256; }
    function widenShift(bytes1 b, uint256 s) external pure returns (bytes32) { return bytes32(b) >> s; }
    function padCheck(bytes1 b) external pure returns (bool) { return bytes32(b) == bytes32(bytes2(b)); }
    function truncChain(bytes32 b) external pure returns (bytes1) { return bytes1(bytes2(bytes4(bytes8(bytes16(b))))); }
    function u16ToB1(uint16 u) external pure returns (bytes1) { return bytes1(uint8(u)); }
    function b4ToU8(bytes4 b) external pure returns (uint8) { return uint8(uint32(b)); }
    function b4ToU8b(bytes4 b) external pure returns (uint8) { return uint8(bytes1(b)); }
    function hashB4(bytes4 b) external pure returns (bytes32) { return keccak256(abi.encodePacked(b)); }
    function encB4(bytes4 b) external pure returns (bytes memory) { return abi.encode(b); }
    function encDecB4(bytes4 b) external pure returns (bytes4) { return abi.decode(abi.encode(b), (bytes4)); }
    function arrB2(bytes2[] memory a) external pure returns (bytes2 r) { for (uint256 i; i < a.length; i++) r |= a[i]; }
    function arrB2cd(bytes2[] calldata a, uint256 i) external pure returns (bytes2) { return a[i]; }
    function arrB2static(bytes2[3] calldata a) external pure returns (bytes32) { return keccak256(abi.encodePacked(a)); }
}
