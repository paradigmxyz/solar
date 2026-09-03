contract StorageBytes {
    bytes b;
    string str;
    bytes[] arr;
    struct S { bytes data; uint256 n; }
    S s;
    mapping(uint256 => bytes) m;

    function pushGrow(uint256 n) external returns (bytes memory, uint256) { for (uint256 i = 0; i < n && i < 40; i++) b.push(bytes1(uint8(i + 1))); return (b, b.length); }
    function pushPop(uint256 n, uint256 k) external returns (bytes memory) { for (uint256 i = 0; i < n && i < 40; i++) b.push(bytes1(uint8(i + 1))); for (uint256 i = 0; i < k && i < 40 && b.length > 0; i++) b.pop(); return b; }
    function assignShortLong(bytes calldata x, bytes calldata y) external returns (bytes memory, bytes memory) { b = x; bytes memory first = b; b = y; return (first, b); }
    function assignThenPush(bytes calldata x) external returns (bytes memory) { b = x; b.push(0xff); return b; }
    function assignThenPop(bytes calldata x) external returns (bytes memory, uint256) { b = x; if (b.length > 0) b.pop(); return (b, b.length); }
    function indexWrite(bytes calldata x, uint256 i, uint8 v) external returns (bytes memory) { b = x; b[i] = bytes1(v); return b; }
    function indexRead(bytes calldata x, uint256 i) external returns (bytes1) { b = x; return b[i]; }
    function lengthOf(bytes calldata x) external returns (uint256) { b = x; return b.length; }
    function deleteBytes(bytes calldata x) external returns (bytes memory, uint256 slot) { b = x; delete b; assembly { slot := sload(b.slot) } return (b, slot); }
    function slotAfterAssign(bytes calldata x) external returns (uint256 slot, uint256 dataSlot) { b = x; assembly { slot := sload(b.slot) mstore(0, b.slot) dataSlot := sload(keccak256(0, 32)) } }
    function stringOps(string calldata x) external returns (string memory, uint256) { str = x; return (str, bytes(str).length); }
    function stringConcatStore(string calldata x, string calldata y) external returns (string memory) { str = string.concat(x, y); return str; }
    function copyStorageToStorage(bytes calldata x) external returns (bytes memory) { b = x; bytes storage r = b; s.data = r; return s.data; }
    function structBytes(bytes calldata x, uint256 n) external returns (bytes memory, uint256) { s = S(x, n); return (s.data, s.n); }
    function arrPushBytes(bytes calldata x, bytes calldata y) external returns (bytes memory, bytes memory, uint256) { arr.push(x); arr.push(y); return (arr[0], arr[1], arr.length); }
    function arrPopBytes(bytes calldata x) external returns (uint256, uint256 slot) { arr.push(x); arr.pop(); assembly { slot := sload(arr.slot) } return (arr.length, slot); }
    function mapBytes(bytes calldata x) external returns (bytes memory) { m[7] = x; return m[7]; }
    function hashStored(bytes calldata x) external returns (bytes32) { b = x; return keccak256(b); }
    function eqStored(bytes calldata x, bytes calldata y) external returns (bool) { b = x; return keccak256(b) == keccak256(y); }
    function memToStorageLong() external returns (bytes memory) { bytes memory t = new bytes(40); for (uint256 i = 0; i < 40; i++) t[i] = bytes1(uint8(i)); b = t; return b; }
    function shrinkViaAssign() external returns (bytes memory, uint256 dataSlot) { b = new bytes(40); b = hex"01"; assembly { mstore(0, b.slot) dataSlot := sload(keccak256(0, 32)) } return (b, dataSlot); }
    function popToShort() external returns (bytes memory, uint256 slot) { b = new bytes(33); b.pop(); b.pop(); assembly { slot := sload(b.slot) } return (b, slot); }
    function pushToLong() external returns (bytes memory, uint256 slot) { b = new bytes(31); b.push(0x01); b.push(0x02); assembly { slot := sload(b.slot) } return (b, slot); }
    function encodeStored(bytes calldata x) external returns (bytes memory) { b = x; return abi.encode(b); }
    function encodePackedStored(bytes calldata x) external returns (bytes memory) { b = x; return abi.encodePacked(b, uint8(1)); }
    function sliceStored(bytes calldata x) external returns (bytes memory) { b = x; bytes memory c = b; return abi.encodePacked(c); }
}
