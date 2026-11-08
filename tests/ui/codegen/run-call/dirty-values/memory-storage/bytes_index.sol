//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_bytes_index => 68
//@ run-call: Cu8_0::t_retdirect_bytes_index => 68
//@ run-call: Cu8_0::t_param_bytes_index => 68
//@ run-call: Cu8_0::t_mem_bytes_index => 68
//@ run-call: Cu8_0::t_local_b32_index => 28
//@ run-call: Cu8_0::t_retdirect_b32_index => 28
//@ run-call: Cu8_0::t_param_b32_index => 28
//@ run-call: Cu8_0::t_mem_b32_index => 28
//@ run-call: Cu64_0::t_local_bytes_index => 68
//@ run-call: Cu64_0::t_retdirect_bytes_index => 68
//@ run-call: Cu64_0::t_param_bytes_index => 68
//@ run-call: Cu64_0::t_mem_bytes_index => 68
//@ run-call: Cu64_0::t_local_b32_index => 28
//@ run-call: Cu64_0::t_retdirect_b32_index => 28
//@ run-call: Cu64_0::t_param_b32_index => 28
//@ run-call: Cu64_0::t_mem_b32_index => 28

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: memory and storage stores, keys, indexes, slices and allocations.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

contract Cu8_0 {
    function _s_retdirect_bytes_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_bytes_index(uint8 pv) internal view  returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[pv])); }
    function _s_retdirect_b32_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_b32_index(uint8 pv) internal view  returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[pv])); }
    function t_local_bytes_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[rx])); }
    function t_retdirect_bytes_index() external view  returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[_s_retdirect_bytes_index()])); }
    function t_param_bytes_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_bytes_index(rx); }
    function t_mem_bytes_index() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[ry])); }
    function t_local_b32_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[rx])); }
    function t_retdirect_b32_index() external view  returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[_s_retdirect_b32_index()])); }
    function t_param_b32_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_b32_index(rx); }
    function t_mem_b32_index() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[ry])); }
}
contract Cu64_0 {
    function _s_retdirect_bytes_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_bytes_index(uint64 pv) internal view  returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[pv])); }
    function _s_retdirect_b32_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_b32_index(uint64 pv) internal view  returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[pv])); }
    function t_local_bytes_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[rx])); }
    function t_retdirect_bytes_index() external view  returns (uint256) { bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[_s_retdirect_bytes_index()])); }
    function t_param_bytes_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_bytes_index(rx); }
    function t_mem_bytes_index() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes memory bm = new bytes(40); bm[27] = 0x44; return uint256(uint8(bm[ry])); }
    function t_local_b32_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[rx])); }
    function t_retdirect_b32_index() external view  returns (uint256) { bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[_s_retdirect_b32_index()])); }
    function t_param_b32_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_b32_index(rx); }
    function t_mem_b32_index() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; bytes32 b32 = bytes32(uint256(0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20)); return uint256(uint8(b32[ry])); }
}
