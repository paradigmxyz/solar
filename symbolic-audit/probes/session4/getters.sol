contract Getters {
    struct S { uint8 a; uint256[] arr; bytes b; string name; mapping(uint256 => uint256) m; uint16 c; }
    struct P { uint256 x; int8 y; bytes4 z; }
    struct Q { P p; uint256[2] fixedArr; bool flag; }
    uint8 public u8; int16 public i16; bool public flag; bytes4 public b4; address public addr; bytes32 public b32; E public en;
    enum E { A, B, C }
    uint256[] public arr; uint8[] public arr8; uint256[3] public fixedArr; uint256[][] public arr2d; uint8[2][] public arr2dFixed;
    bytes public bs; string public str; bytes[] public bsArr; string[] public strArr;
    mapping(uint256 => uint256) public m; mapping(address => mapping(bytes4 => int8)) public mm; mapping(uint256 => uint256[]) public marr; mapping(string => uint256) public mstr; mapping(bytes => bytes) public mbytes;
    S public s; P public p; Q public q; P[] public parr; S[] public sarr; mapping(uint256 => P) public mp; mapping(uint256 => S) public ms; mapping(uint256 => Q) public mq; Q[2] public qfixed;
    uint256 public constant C1 = 2 ** 200 + 1; string public constant C2 = "a constant string longer than thirty-two bytes."; bytes4 public constant C3 = 0x01020304; address public constant C4 = address(0xdead); bytes public constant C5 = hex"aabbcc"; int8 public constant C6 = -5; bool public constant C7 = true; E public constant C8 = E.C;
    function setScalars(uint8 a, int16 b, bool c, bytes4 d, address e, bytes32 f, uint8 g) external { require(g < 3); u8 = a; i16 = b; flag = c; b4 = d; addr = e; b32 = f; en = E(g); }
    function setScalarsDirty(uint256 raw) external { assembly { sstore(u8.slot, raw) sstore(addr.slot, raw) sstore(b32.slot, raw) } }
    function setArr(uint256 n) external { require(n < 5); for (uint256 i; i < n; i++) { arr.push(i + 1); arr8.push(uint8(i + 1)); } fixedArr[1] = n; }
    function setArr2d(uint256 v) external { arr2d.push(); arr2d[0].push(v); arr2d.push(); arr2d[1].push(v + 1); arr2d[1].push(v + 2); arr2dFixed.push([uint8(v), uint8(v + 1)]); }
    function setBytes(bytes calldata d, string calldata t) external { bs = d; str = t; bsArr.push(d); bsArr.push(hex"ff"); strArr.push(t); strArr.push(""); }
    function setMaps(address a, bytes4 k, int8 v, uint256 n) external { m[n] = n * 2; mm[a][k] = v; marr[n].push(n); marr[n].push(n + 1); mstr["k"] = n; mbytes[hex"01"] = hex"0203"; }
    function setStructs(uint8 a, int8 y, bytes4 z, bytes calldata b) external { s.a = a; s.arr.push(1); s.b = b; s.name = "nm"; s.m[1] = 1; s.c = 0xbeef; p = P(uint256(a), y, z); q.p = p; q.fixedArr[1] = 7; q.flag = true; parr.push(p); parr.push(P(1, -1, 0xffffffff)); sarr.push(); sarr[0].a = a; sarr[0].c = 3; mp[5] = p; ms[6].a = a; ms[6].name = "long name for a struct member, over 32 bytes"; mq[7].p = p; mq[7].flag = true; qfixed[1].p.y = y; }
    function setStructsDirty(uint256 raw) external { assembly { sstore(p.slot, raw) sstore(add(p.slot, 1), raw) sstore(add(p.slot, 2), raw) sstore(s.slot, raw) } }
    function setArrDirty(uint256 raw) external { arr8.push(0); arr8.push(0); uint256 sl; assembly { mstore(0, arr8.slot) sl := keccak256(0, 32) sstore(sl, raw) } }
}
