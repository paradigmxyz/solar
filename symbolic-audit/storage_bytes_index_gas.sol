contract G {
    bytes a; bytes b; uint8[] u; uint256[] w;
    function fill(uint256 n) external { bytes memory x = new bytes(n); for (uint256 i; i < n; i++) x[i] = bytes1(uint8(i)); a = x; }
    function fillU(uint256 n) external { for (uint256 i; i < n; i++) u.push(uint8(i)); }
    function fillW(uint256 n) external { for (uint256 i; i < n; i++) w.push(i); }
    function readOne(uint256 i) external view returns (bytes1) { return a[i]; }
    function readAll() external view returns (uint256 s) { for (uint256 i; i < a.length; i++) s += uint8(a[i]); }
    function readAllU() external view returns (uint256 s) { for (uint256 i; i < u.length; i++) s += u[i]; }
    function readAllW() external view returns (uint256 s) { for (uint256 i; i < w.length; i++) s += w[i]; }
    function writeAll() external { for (uint256 i; i < a.length; i++) a[i] = bytes1(uint8(i + 1)); }
    function copyStorage() external { b = a; }
    function lenOnly() external view returns (uint256) { return a.length; }
    function toMem() external view returns (uint256) { bytes memory m = a; return m.length; }
    function memLoop(uint256 n) external pure returns (uint256 s) { bytes memory x = new bytes(n); for (uint256 i; i < n; i++) { x[i] = bytes1(uint8(i)); } for (uint256 i; i < n; i++) s += uint8(x[i]); }
}
