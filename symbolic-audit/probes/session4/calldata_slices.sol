contract CalldataSlices {
    function tail(bytes calldata d) external pure returns (bytes memory) { return d[1:]; }
    function head(bytes calldata d) external pure returns (bytes memory) { return d[:2]; }
    function mid(bytes calldata d, uint256 i, uint256 j) external pure returns (bytes memory) { return d[i:j]; }
    function midLen(bytes calldata d, uint256 i, uint256 j) external pure returns (uint256) { bytes calldata t = d[i:j]; return t.length; }
    function sliceOfSlice(bytes calldata d, uint256 i, uint256 j) external pure returns (bytes memory) { return d[i:][:j]; }
    function sliceIdx(bytes calldata d, uint256 i, uint256 k) external pure returns (bytes1) { return d[i:][k]; }
    function sliceB4(bytes calldata d) external pure returns (bytes4) { return bytes4(d[:4]); }
    function sliceB4Short(bytes calldata d, uint256 i) external pure returns (bytes4) { return bytes4(d[i:]); }
    function sliceHash(bytes calldata d, uint256 i) external pure returns (bytes32) { return keccak256(d[i:]); }
    function sliceDecode(bytes calldata d) external pure returns (uint256, address) { return abi.decode(d[4:], (uint256, address)); }
    function sliceDecodeArr(bytes calldata d) external pure returns (uint256[] memory) { return abi.decode(d[4:], (uint256[])); }
    function sliceToInternal(bytes calldata d, uint256 i) external pure returns (uint256) { return _sum(d[i:]); }
    function _sum(bytes calldata s) internal pure returns (uint256 r) { for (uint256 k; k < s.length; k++) r += uint8(s[k]); }
    function sliceEncode(bytes calldata d, uint256 i) external pure returns (bytes memory) { return abi.encode(d[i:]); }
    function slicePacked(bytes calldata d, uint256 i) external pure returns (bytes memory) { return abi.encodePacked(d[i:], d[:i]); }
    function sliceEmpty(bytes calldata d) external pure returns (uint256) { bytes calldata t = d[d.length:]; return t.length; }
    function sliceEq(bytes calldata d) external pure returns (bool) { return keccak256(d[:d.length / 2]) == keccak256(d[d.length / 2:]); }
    function arrTail(uint256[] calldata a) external pure returns (uint256[] memory) { return a[1:]; }
    function arrHead(uint256[] calldata a, uint256 n) external pure returns (uint256[] memory) { return a[:n]; }
    function arrMid(uint256[] calldata a, uint256 i, uint256 j) external pure returns (uint256) { uint256[] calldata t = a[i:j]; return t.length; }
    function arrSliceIdx(uint256[] calldata a, uint256 i, uint256 k) external pure returns (uint256) { return a[i:][k]; }
    function arrSliceSum(uint256[] calldata a, uint256 i) external pure returns (uint256 s) { uint256[] calldata t = a[i:]; for (uint256 k; k < t.length; k++) s += t[k]; }
    function arrSliceEncode(uint256[] calldata a, uint256 i) external pure returns (bytes memory) { return abi.encode(a[i:]); }
    function arrSliceInternal(uint256[] calldata a, uint256 i) external pure returns (uint256) { return _first(a[i:]); }
    function _first(uint256[] calldata s) internal pure returns (uint256) { return s.length > 0 ? s[0] : 0; }
    function u8SliceIdx(uint8[] calldata a, uint256 i, uint256 k) external pure returns (uint8) { return a[i:][k]; }
    function u8SliceCopy(uint8[] calldata a, uint256 i) external pure returns (uint8[] memory) { return a[i:]; }
    function strSlice(string calldata s, uint256 i) external pure returns (string memory) { return string(bytes(s)[i:]); }
    function strSliceLen(string calldata s, uint256 i, uint256 j) external pure returns (uint256) { bytes calldata t = bytes(s)[i:j]; return t.length; }
    function structArrSlice(P[] calldata ps, uint256 i) external pure returns (uint256) { P[] calldata t = ps[i:]; return t.length > 0 ? t[0].x : 0; }
    struct P { uint256 x; uint256 y; }
    function assignSlice(bytes calldata d, uint256 i) external pure returns (uint256) { bytes calldata s = d; s = s[i:]; s = s[:s.length]; return s.length; }
    function sliceLoopShrink(bytes calldata d) external pure returns (uint256 n) { bytes calldata s = d; while (s.length > 0) { s = s[1:]; n++; } }
    function sliceB32(bytes calldata d, uint256 i) external pure returns (bytes32) { return bytes32(d[i:i + 32]); }
    function sliceB32Short(bytes calldata d, uint256 i) external pure returns (bytes32) { return bytes32(d[i:]); }
    function sliceCmpLen(bytes calldata d, uint256 i) external pure returns (bool) { bytes calldata t = d[i:]; return t.length == d.length - i; }
    function bytesCdTwoParams(bytes calldata a, bytes calldata b, uint256 i) external pure returns (bytes memory) { return bytes.concat(a[i:], b[:i]); }
    function sliceReturnMulti(bytes calldata d, uint256 i) external pure returns (bytes memory, bytes memory) { return (d[:i], d[i:]); }
    function sliceOffsetAsm(bytes calldata d, uint256 i) external pure returns (uint256 off, uint256 len) { bytes calldata s = d[i:]; assembly { off := s.offset len := s.length } off -= 4; }
}
