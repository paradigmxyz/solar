//@ revisions: none gas size
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: memAssign => [3, 1, 2]
//@ run-call: memBytesAssign => [3, 1, 2]
//@ run-call: memCompound => [3, 1, 2]
//@ run-call: memIncrement => [1, 2]
//@ run-call: memDelete => [1, 2]
//@ run-call: storageAssign => [3, 1, 2]
//@ run-call: storageCompound => [3, 1, 2]
//@ run-call: storageIncrement => [1, 2]
//@ run-call: storageDelete => [1, 2]
//@ run-call: storageBytesAssign => [3, 1, 2]
//@ run-call: mappingAssign => [3, 1, 2]
//@ run-call: mappingCompound => [3, 1, 2]
//@ run-call: nestedMappingAssign => [4, 1, 2, 3]

// An assignment evaluates its right-hand side, then the base and the index of
// its target, as in solc.
contract C {
    uint256[] log;
    uint256[] arr;
    uint256[3] fixedArr;
    bytes sbytes;
    mapping(uint256 => uint256) map;
    mapping(uint256 => mapping(uint256 => uint256)) nested;

    function t(uint256 k) internal returns (uint256) { log.push(k); return k; }
    function mk() internal returns (uint256[] memory a) { log.push(1); a = new uint256[](3); }
    function mkFixed() internal returns (uint256[3] memory a) { log.push(1); }
    function mkBytes() internal returns (bytes memory a) { log.push(1); a = new bytes(3); }
    function mkFb() internal returns (bytes32 a) { log.push(1); a = bytes32(uint256(5)); }
    function s() internal returns (uint256[] storage a) { log.push(1); a = arr; }
    function sFixed() internal returns (uint256[3] storage a) { log.push(1); a = fixedArr; }
    function sb() internal returns (bytes storage a) { log.push(1); a = sbytes; }
    function mp() internal returns (mapping(uint256 => uint256) storage a) { log.push(1); a = map; }
    function mp2() internal returns (mapping(uint256 => mapping(uint256 => uint256)) storage a) { log.push(1); a = nested; }
    function cd(uint256[] calldata a) internal returns (uint256[] calldata) { log.push(1); return a; }
    function cdb(bytes calldata a) internal returns (bytes calldata) { log.push(1); return a; }
    function cdn(uint256[][] calldata a) internal returns (uint256[][] calldata) { log.push(1); return a; }

    function init() internal {
        delete log;
        delete arr;
        delete sbytes;
        arr.push(0); arr.push(0); arr.push(0);
        sbytes = new bytes(3);
    }

    function memAssign() external returns (uint256[] memory) { init(); mk()[t(2)] = t(3); return log; }
    function memBytesAssign() external returns (uint256[] memory) { init(); mkBytes()[t(2)] = bytes1(uint8(t(3))); return log; }
    function memCompound() external returns (uint256[] memory) { init(); mk()[t(2)] += t(3); return log; }
    function memIncrement() external returns (uint256[] memory) { init(); mk()[t(2)]++; return log; }
    function memDelete() external returns (uint256[] memory) { init(); delete mk()[t(2)]; return log; }
    function storageAssign() external returns (uint256[] memory) { init(); s()[t(2)] = t(3); return log; }
    function storageCompound() external returns (uint256[] memory) { init(); s()[t(2)] += t(3); return log; }
    function storageIncrement() external returns (uint256[] memory) { init(); s()[t(2)]++; return log; }
    function storageDelete() external returns (uint256[] memory) { init(); delete s()[t(2)]; return log; }
    function storageBytesAssign() external returns (uint256[] memory) { init(); sb()[t(2)] = bytes1(uint8(t(3))); return log; }
    function mappingAssign() external returns (uint256[] memory) { init(); mp()[t(2)] = t(3); return log; }
    function mappingCompound() external returns (uint256[] memory) { init(); mp()[t(2)] += t(3); return log; }
    function nestedMappingAssign() external returns (uint256[] memory) { init(); mp2()[t(2)][t(3)] = t(4); return log; }
}
