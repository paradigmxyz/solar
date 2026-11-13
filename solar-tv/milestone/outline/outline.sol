contract Outline {
    uint256 s;
    function a(uint256 x, uint256 y) external { unchecked { s = ((x << 3) ^ (y >> 7)) | (x & y); } }
    function b(uint256 x, uint256 y) external view returns (uint256) { unchecked { return (((x << 3) ^ (y >> 7)) | (x & y)) + s; } }
    function c(uint256 x, uint256 y) external pure returns (uint256) { unchecked { return (((x << 3) ^ (y >> 7)) | (x & y)) * 5; } }
}
