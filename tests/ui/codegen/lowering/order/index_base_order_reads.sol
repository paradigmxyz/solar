//@ revisions: none gas size
//@[none] compile-flags: -Onone
//@[gas] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@ run-call: memRead => [1, 2]
//@ run-call: memFixedRead => [1, 2]
//@ run-call: memBytesRead => [1, 2]
//@ run-call: fixedBytesRead => [1, 2]
//@ run-call: storageRead => [1, 2]
//@ run-call: storageFixedRead => [1, 2]
//@ run-call: storageBytesRead => [1, 2]
//@ run-call: mappingRead => [1, 2]
//@ run-call: nestedMappingRead => [1, 2, 3]
//@ run-call: calldataRead [0,0,0] => [1, 2]
//@ run-call: calldataBytesRead 0x000000 => [1, 2]
//@ run-call: calldataNestedRead [[0,0,0],[0,0,0],[0,0,0]] => [1, 1, 2]
//@ run-call: calldataSliceRead [0,0,0,0] => [1, 0, 3, 2]
//@ run-call: calldataBytesSliceRead 0x00000000 => [1, 0, 3, 2]
//@ run-call: memNestedRead => [1, 1, 2]

// The base of an index access is evaluated before the index: the first helper call
// logs 1 and the index expression logs the following values, as in solc.
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

    function memRead() external returns (uint256[] memory) { init(); mk()[t(2)]; return log; }
    function memFixedRead() external returns (uint256[] memory) { init(); mkFixed()[t(2)]; return log; }
    function memBytesRead() external returns (uint256[] memory) { init(); mkBytes()[t(2)]; return log; }
    function fixedBytesRead() external returns (uint256[] memory) { init(); mkFb()[t(2)]; return log; }
    function storageRead() external returns (uint256[] memory) { init(); s()[t(2)]; return log; }
    function storageFixedRead() external returns (uint256[] memory) { init(); sFixed()[t(2)]; return log; }
    function storageBytesRead() external returns (uint256[] memory) { init(); sb()[t(2)]; return log; }
    function mappingRead() external returns (uint256[] memory) { init(); mp()[t(2)]; return log; }
    function nestedMappingRead() external returns (uint256[] memory) { init(); mp2()[t(2)][t(3)]; return log; }
    function calldataRead(uint256[] calldata a) external returns (uint256[] memory) { init(); cd(a)[t(2)]; return log; }
    function calldataBytesRead(bytes calldata a) external returns (uint256[] memory) { init(); cdb(a)[t(2)]; return log; }
    function calldataNestedRead(uint256[][] calldata a) external returns (uint256[] memory) { init(); cdn(a)[t(1)][t(2)]; return log; }
    function calldataSliceRead(uint256[] calldata a) external returns (uint256[] memory) { init(); cd(a)[t(0):t(3)][t(2)]; return log; }
    function calldataBytesSliceRead(bytes calldata a) external returns (uint256[] memory) { init(); cdb(a)[t(0):t(3)][t(2)]; return log; }
    function memNestedRead() external returns (uint256[] memory) { init(); mkNested()[t(1)][t(2)]; return log; }
    function mkNested() internal returns (uint256[][] memory a) { log.push(1); a = new uint256[][](3); a[1] = new uint256[](3); }

}
