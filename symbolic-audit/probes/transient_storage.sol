contract TransientStorage {
    uint256 transient t;
    uint8 transient t8;
    bool transient tb;
    address transient ta;
    bytes4 transient tbs;
    uint256 s;
    function set(uint256 v) external returns (uint256) { t = v; return t; }
    function setNarrow(uint256 v) external returns (uint8, bool, address, bytes4) { uint8 x; bool b; address a; bytes4 bs; assembly { x := v b := v a := v bs := v } t8 = x; tb = b; ta = a; tbs = bs; return (t8, tb, ta, tbs); }
    function slots(uint256 v) external returns (uint256, uint256) { t8 = uint8(v); tb = v > 5; uint256 w; uint256 w2; assembly { w := tload(t8.slot) w2 := tload(t.slot) } return (w, w2); }
    function inc(uint256 v) external returns (uint256) { t = v; t += 1; t++; ++t; t -= 1; return t; }
    function del(uint256 v) external returns (uint256) { t = v; delete t; return t; }
    function mix(uint256 v) external returns (uint256) { t = v; s = v + 1; return t + s; }
    function readUninit() external view returns (uint256, uint8, bool) { return (t, t8, tb); }
    function transientThenReturn(uint256 v) external returns (uint256) { t = v; return this.readT(); }
    function readT() external view returns (uint256) { return t; }
    function branchy(uint256 v) external returns (uint256) { if (v % 2 == 0) { t = v; } else { t8 = uint8(v); } return t + t8; }
}
