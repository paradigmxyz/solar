library L {
    struct Set { mapping(uint256 => uint256) idx; }
    struct Plain { uint256 a; }
    function withMapping(Set storage s) external view returns (uint256) { return s.idx[0]; }
    function plainStruct(Plain storage p) external view returns (uint256) { return p.a; }
    function mappingParam(mapping(uint256 => uint256) storage m) external view returns (uint256) { return m[0]; }
    function arrParam(uint256[] storage a) external view returns (uint256) { return a.length; }
}
