contract StringsUnicode {
    string s; bytes b;
    function esc() external pure returns (bytes memory) { return bytes("a\nb\tc\\d\"e'f\x41B"); }
    function escLen() external pure returns (uint256, uint256, uint256) { return (bytes("\n").length, bytes("\x00\xff").length, bytes(unicode"héllo").length); }
    function uni() external pure returns (bytes memory) { return bytes(unicode"héllo wörld 🎉"); }
    function uniLen() external pure returns (uint256) { return bytes(unicode"🎉").length; }
    function hexLit() external pure returns (bytes memory, uint256) { bytes memory h = hex"00_01_ff"; return (h, h.length); }
    function hexLitEmpty() external pure returns (uint256) { bytes memory h = hex""; return h.length; }
    function concatLit() external pure returns (string memory) { return "abc" "def"; }
    function concatLitHex() external pure returns (bytes memory) { return hex"01" hex"02"; }
    function strConcat(string calldata a) external pure returns (string memory) { return string.concat("[", a, "]"); }
    function strConcatEmpty() external pure returns (string memory, uint256) { string memory r = string.concat("", ""); return (r, bytes(r).length); }
    function strConcatMany(string calldata a) external pure returns (string memory) { return string.concat(a, a, a, a, a); }
    function strLenCd(string calldata a) external pure returns (uint256) { return bytes(a).length; }
    function strLenMem(string memory a) external pure returns (uint256) { return bytes(a).length; }
    function strIdxCd(string calldata a, uint256 i) external pure returns (bytes1) { return bytes(a)[i]; }
    function strIdxMem(string memory a, uint256 i) external pure returns (bytes1) { return bytes(a)[i]; }
    function strEq(string calldata a, string calldata c) external pure returns (bool) { return keccak256(bytes(a)) == keccak256(bytes(c)); }
    function strEqLit(string calldata a) external pure returns (bool) { return keccak256(bytes(a)) == keccak256("hello"); }
    function strStorage(string calldata a) external returns (string memory, uint256) { s = a; return (s, bytes(s).length); }
    function strStoragePush(string calldata a) external returns (uint256) { s = a; bytes(s).push("x"); return bytes(s).length; }
    function strStorageIdx(string calldata a, uint256 i) external returns (bytes1) { s = a; return bytes(s)[i]; }
    function strStorageConcat(string calldata a) external returns (string memory) { s = a; s = string.concat(s, s); return s; }
    function strStorageLit() external returns (string memory) { s = "a literal that is longer than thirty-two bytes for sure!"; return s; }
    function strStorageLitShort() external returns (string memory, uint256 raw) { s = "short"; assembly { raw := sload(s.slot) } return (s, raw); }
    function strStorageLit31() external returns (uint256 raw) { s = "exactly thirty-one bytes long!!"; assembly { raw := sload(s.slot) } }
    function strStorageLit32() external returns (uint256 raw, uint256 data) { s = "exactly thirty-two bytes long!!!"; assembly { raw := sload(s.slot) mstore(0, s.slot) data := sload(keccak256(0, 32)) } }
    function strStorageDelete(string calldata a) external returns (uint256, uint256 raw) { s = a; delete s; assembly { raw := sload(s.slot) } return (bytes(s).length, raw); }
    function strStorageShrink(string calldata a) external returns (uint256, uint256 raw, uint256 data) { s = a; s = "ab"; assembly { raw := sload(s.slot) mstore(0, s.slot) data := sload(keccak256(0, 32)) } return (bytes(s).length, raw, data); }
    function strStorageGrow(string calldata a) external returns (uint256, uint256 raw) { s = "ab"; s = a; assembly { raw := sload(s.slot) } return (bytes(s).length, raw); }
    function strStorageToMem(string calldata a) external returns (uint256) { s = a; string memory m = s; bytes(m)[0] = "Z"; return bytes(m).length + (bytes(s)[0] == "Z" ? 1000 : 0); }
    function strStorageCopy(string calldata a) external returns (string memory) { s = a; string storage p = s; return p; }
    function strMemWrite(string memory a, uint256 i) external pure returns (string memory) { bytes(a)[i] = "!"; return a; }
    function strLitBytesN() external pure returns (bytes32, bytes8, bytes1) { return ("thirty-two byte string literal!!", "eight by", "a"); }
    function strLitBytesNShort() external pure returns (bytes32) { return "short"; }
    function strLitBytesNUni() external pure returns (bytes5) { return unicode"héé"; }
    function strLitBytesNHex() external pure returns (bytes4) { return hex"deadbeef"; }
    function strLitBytesNHexShort() external pure returns (bytes4) { return hex"de"; }
    function strLitEmpty() external pure returns (string memory, bytes memory, uint256) { return ("", "", bytes("").length); }
    function strLitCmpBytes() external pure returns (bool, bool) { return (bytes4("abcd") == 0x61626364, bytes1("a") == 0x61); }
    function strLitBytes(uint256 i) external pure returns (bytes1) { return bytes("hello")[i]; }
    function strLitToBytesLen() external pure returns (uint256) { return bytes("hello world, longer than 32 bytes for sure.").length; }
    function strLitEnc() external pure returns (bytes memory) { return abi.encode("hi", unicode"ü"); }
    function strLitPacked() external pure returns (bytes memory) { return abi.encodePacked("hi", unicode"ü", hex"00"); }
    function strLitHash() external pure returns (bytes32) { return keccak256("hello world, longer than 32 bytes for sure."); }
    function strLitHashBytes() external pure returns (bytes32) { return keccak256(bytes("hello")); }
    function strArr(string[] calldata a, uint256 i) external pure returns (string memory) { return a[i]; }
    function strArrMem(uint256 n) external pure returns (string[] memory r) { require(n < 4); r = new string[](n); for (uint256 k; k < n; k++) r[k] = k == 0 ? "" : k == 1 ? "one" : "a long string of at least thirty-three bytes"; }
    function strArrStorage(string calldata a) external returns (string memory, uint256) { sarr.push(a); sarr.push("lit"); sarr.push(); return (sarr[0], bytes(sarr[1]).length + sarr.length); }
    string[] sarr;
    function strStructMem(string calldata a) external pure returns (uint256) { S memory x = S(a, 1); return bytes(x.name).length + x.v; }
    struct S { string name; uint256 v; }
    function strStructStorage(string calldata a) external returns (string memory) { ss = S(a, 2); ss.name = string.concat(ss.name, "!"); return ss.name; }
    S ss;
    function strMapKey(string calldata a) external returns (uint256) { sm[a] = 5; return sm[a] + sm[string.concat(a, "")] + sm["other"]; }
    mapping(string => uint256) sm;
    function bytesToStrToBytes(bytes calldata d) external pure returns (bytes memory) { return bytes(string(d)); }
    function bytesMemToStr(bytes memory d) external pure returns (string memory) { return string(d); }
    function strLitLongLocal() external pure returns (uint256, bytes1) { string memory l = "this literal is much longer than thirty two bytes and spans a bit"; return (bytes(l).length, bytes(l)[40]); }
    function strLitInCond(bool c) external pure returns (string memory) { return c ? "yes" : "no, definitely not, this one is a long one!"; }
    function strLitReturnMulti() external pure returns (string memory, string memory, bytes memory) { return ("a", "bb", hex"cc"); }
    function bytes1Lit() external pure returns (bytes1, bytes1, bytes1) { return ("a", hex"ff", 0x01); }
    function strCdToStorageAndBack(string calldata a) external returns (bool) { s = a; b = bytes(a); return keccak256(bytes(s)) == keccak256(b); }
    function strLenCmp(string calldata a, string calldata c) external pure returns (bool) { return bytes(a).length < bytes(c).length; }
}
