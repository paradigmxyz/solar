//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cu8_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cu8_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cu8_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cu8_0::t_local_mapping_key_read => 9
//@ run-call: Cu8_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cu8_0::t_param_mapping_key_read => 9
//@ run-call: Cu8_0::t_mem_mapping_key_read => 9
//@ run-call: Cu8_0::t_local_mapping_value_raw => 27
//@ run-call: Cu8_0::t_retdirect_mapping_value_raw => 27
//@ run-call: Cu8_0::t_param_mapping_value_raw => 27
//@ run-call: Cu8_0::t_mem_mapping_value_raw => 27
//@ run-call: Cu64_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cu64_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cu64_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cu64_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cu64_0::t_local_mapping_key_read => 9
//@ run-call: Cu64_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cu64_0::t_param_mapping_key_read => 9
//@ run-call: Cu64_0::t_mem_mapping_key_read => 9
//@ run-call: Cu64_0::t_local_mapping_value_raw => 27
//@ run-call: Cu64_0::t_retdirect_mapping_value_raw => 27
//@ run-call: Cu64_0::t_param_mapping_value_raw => 27
//@ run-call: Cu64_0::t_mem_mapping_value_raw => 27
//@ run-call: Ci8_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Ci8_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Ci8_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Ci8_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Ci8_0::t_local_mapping_key_read => 9
//@ run-call: Ci8_0::t_retdirect_mapping_key_read => 9
//@ run-call: Ci8_0::t_param_mapping_key_read => 9
//@ run-call: Ci8_0::t_mem_mapping_key_read => 9
//@ run-call: Ci8_0::t_local_mapping_value_raw => 253
//@ run-call: Ci8_0::t_retdirect_mapping_value_raw => 253
//@ run-call: Ci8_0::t_param_mapping_value_raw => 253
//@ run-call: Ci8_0::t_mem_mapping_value_raw => 253
//@ run-call: Ci24_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Ci24_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Ci24_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Ci24_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Ci24_0::t_local_mapping_key_read => 9
//@ run-call: Ci24_0::t_retdirect_mapping_key_read => 9
//@ run-call: Ci24_0::t_param_mapping_key_read => 9
//@ run-call: Ci24_0::t_mem_mapping_key_read => 9
//@ run-call: Ci24_0::t_local_mapping_value_raw => 16777213
//@ run-call: Ci24_0::t_retdirect_mapping_value_raw => 16777213
//@ run-call: Ci24_0::t_param_mapping_value_raw => 16777213
//@ run-call: Ci24_0::t_mem_mapping_value_raw => 16777213
//@ run-call: Caddr_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Caddr_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Caddr_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Caddr_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Caddr_0::t_local_mapping_key_read => 9
//@ run-call: Caddr_0::t_retdirect_mapping_key_read => 9
//@ run-call: Caddr_0::t_param_mapping_key_read => 9
//@ run-call: Caddr_0::t_mem_mapping_key_read => 9
//@ run-call: Caddr_0::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Caddrp_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Caddrp_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Caddrp_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Caddrp_0::t_local_mapping_key_read => 9
//@ run-call: Caddrp_0::t_retdirect_mapping_key_read => 9
//@ run-call: Caddrp_0::t_param_mapping_key_read => 9
//@ run-call: Caddrp_0::t_mem_mapping_key_read => 9
//@ run-call: Caddrp_0::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cctr_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cctr_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cctr_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cctr_0::t_local_mapping_key_read => 9
//@ run-call: Cctr_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cctr_0::t_param_mapping_key_read => 9
//@ run-call: Cctr_0::t_mem_mapping_key_read => 9
//@ run-call: Cctr_0::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cbool_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cbool_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cbool_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cbool_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cbool_0::t_local_mapping_key_read => 9
//@ run-call: Cbool_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cbool_0::t_param_mapping_key_read => 9
//@ run-call: Cbool_0::t_mem_mapping_key_read => 9
//@ run-call: Cbool_0::t_local_mapping_value_raw => 1
//@ run-call: Cbool_0::t_retdirect_mapping_value_raw => 1
//@ run-call: Cbool_0::t_param_mapping_value_raw => 1
//@ run-call: Cbool_0::t_mem_mapping_value_raw => 1
//@ run-call: Cb1_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb1_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb1_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb1_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb1_0::t_local_mapping_key_read => 9
//@ run-call: Cb1_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cb1_0::t_param_mapping_key_read => 9
//@ run-call: Cb1_0::t_mem_mapping_key_read => 9
//@ run-call: Cb1_0::t_local_mapping_value_raw => 171
//@ run-call: Cb1_0::t_retdirect_mapping_value_raw => 171
//@ run-call: Cb1_0::t_param_mapping_value_raw => 171
//@ run-call: Cb1_0::t_mem_mapping_value_raw => 171
//@ run-call: Cb4_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb4_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb4_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb4_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb4_0::t_local_mapping_key_read => 9
//@ run-call: Cb4_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cb4_0::t_param_mapping_key_read => 9
//@ run-call: Cb4_0::t_mem_mapping_key_read => 9
//@ run-call: Cb4_0::t_local_mapping_value_raw => 3735928559
//@ run-call: Cb4_0::t_retdirect_mapping_value_raw => 3735928559
//@ run-call: Cb4_0::t_param_mapping_value_raw => 3735928559
//@ run-call: Cb4_0::t_mem_mapping_value_raw => 3735928559
//@ run-call: Cb20_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cb20_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cb20_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cb20_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cb20_0::t_local_mapping_key_read => 9
//@ run-call: Cb20_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cb20_0::t_param_mapping_key_read => 9
//@ run-call: Cb20_0::t_mem_mapping_key_read => 9
//@ run-call: Cb20_0::t_local_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_mapping_value_raw => 103929005307130220006098923584552504982110632080
//@ run-call-fail: Cen_0::t_local_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mapping_value_key_dirty => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mapping_key_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mapping_value_raw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_mapping_value_key_dirty => 5
//@ run-call: Cud_0::t_retdirect_mapping_value_key_dirty => 5
//@ run-call: Cud_0::t_param_mapping_value_key_dirty => 5
//@ run-call: Cud_0::t_mem_mapping_value_key_dirty => 5
//@ run-call: Cud_0::t_local_mapping_key_read => 9
//@ run-call: Cud_0::t_retdirect_mapping_key_read => 9
//@ run-call: Cud_0::t_param_mapping_key_read => 9
//@ run-call: Cud_0::t_mem_mapping_key_read => 9
//@ run-call: Cud_0::t_local_mapping_value_raw => 27
//@ run-call: Cud_0::t_retdirect_mapping_value_raw => 27
//@ run-call: Cud_0::t_param_mapping_value_raw => 27
//@ run-call: Cud_0::t_mem_mapping_value_raw => 27

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
    mapping(uint8 => uint256) mp;
    mapping(uint256 => uint8) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_key_dirty(uint8 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_key_read(uint8 pv) internal   returns (uint256) { mp[uint8(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_raw(uint8 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mp[rx] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mp[ry] = 5; if (mp[uint8(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mp[uint8(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[uint8(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mp[uint8(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cu64_0 {
    mapping(uint64 => uint256) mp;
    mapping(uint256 => uint64) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_value_key_dirty(uint64 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_key_read(uint64 pv) internal   returns (uint256) { mp[uint64(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mapping_value_raw(uint64 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mp[rx] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mp[ry] = 5; if (mp[uint64(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mp[uint64(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[uint64(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mp[uint64(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Ci8_0 {
    mapping(int8 => uint256) mp;
    mapping(uint256 => int8) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_value_key_dirty(int8 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_key_read(int8 pv) internal   returns (uint256) { mp[int8(-3)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mapping_value_raw(int8 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mp[rx] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mp[ry] = 5; if (mp[int8(-3)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mp[int8(-3)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[int8(-3)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mp[int8(-3)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Ci24_0 {
    mapping(int24 => uint256) mp;
    mapping(uint256 => int24) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_value_key_dirty(int24 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_key_read(int24 pv) internal   returns (uint256) { mp[int24(-3)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mapping_value_raw(int24 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mp[rx] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mp[ry] = 5; if (mp[int24(-3)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mp[int24(-3)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[int24(-3)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mp[int24(-3)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Caddr_0 {
    mapping(address => uint256) mp;
    mapping(uint256 => address) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(address pv) internal   returns (uint256) { mp[pv] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(address pv) internal   returns (uint256) { mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(address pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mp[ry] = 5; if (mp[address(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mp[address(0x1234567890123456789012345678901234567890)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Caddrp_0 {
    mapping(address => uint256) mp;
    mapping(uint256 => address payable) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(address payable pv) internal   returns (uint256) { mp[pv] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(address payable pv) internal   returns (uint256) { mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(address payable pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mp[ry] = 5; if (mp[payable(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mp[payable(address(0x1234567890123456789012345678901234567890))] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cctr_0 {
    mapping(IC => uint256) mp;
    mapping(uint256 => IC) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_key_dirty(IC pv) internal   returns (uint256) { mp[pv] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_key_read(IC pv) internal   returns (uint256) { mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_mapping_value_raw(IC pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[rx] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mp[ry] = 5; if (mp[IC(address(0x1234567890123456789012345678901234567890))] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mp[IC(address(0x1234567890123456789012345678901234567890))] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cbool_0 {
    mapping(bool => uint256) mp;
    mapping(uint256 => bool) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_value_key_dirty(bool pv) internal   returns (uint256) { mp[pv] = 5; if (mp[true] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_key_read(bool pv) internal   returns (uint256) { mp[true] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mapping_value_raw(bool pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { bool rx; assembly { rx := 0x100 } mp[rx] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mp[ry] = 5; if (mp[true] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { bool rx; assembly { rx := 0x100 } mp[true] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[true] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mp[true] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { bool rx; assembly { rx := 0x100 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cb1_0 {
    mapping(bytes1 => uint256) mp;
    mapping(uint256 => bytes1) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_value_key_dirty(bytes1 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_key_read(bytes1 pv) internal   returns (uint256) { mp[bytes1(0xab)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mapping_value_raw(bytes1 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mp[rx] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mp[ry] = 5; if (mp[bytes1(0xab)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mp[bytes1(0xab)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[bytes1(0xab)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mp[bytes1(0xab)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cb4_0 {
    mapping(bytes4 => uint256) mp;
    mapping(uint256 => bytes4) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_value_key_dirty(bytes4 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_key_read(bytes4 pv) internal   returns (uint256) { mp[bytes4(0xdeadbeef)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mapping_value_raw(bytes4 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mp[rx] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mp[ry] = 5; if (mp[bytes4(0xdeadbeef)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mp[bytes4(0xdeadbeef)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[bytes4(0xdeadbeef)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mp[bytes4(0xdeadbeef)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cb20_0 {
    mapping(bytes20 => uint256) mp;
    mapping(uint256 => bytes20) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_value_key_dirty(bytes20 pv) internal   returns (uint256) { mp[pv] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_key_read(bytes20 pv) internal   returns (uint256) { mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mapping_value_raw(bytes20 pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mp[rx] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mp[ry] = 5; if (mp[bytes20(0x1234567890123456789012345678901234567890)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mp[bytes20(0x1234567890123456789012345678901234567890)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cen_0 {
    mapping(E => uint256) mp;
    mapping(uint256 => E) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_value_key_dirty(E pv) internal   returns (uint256) { mp[pv] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_key_read(E pv) internal   returns (uint256) { mp[E.B] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mapping_value_raw(E pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { E rx; assembly { rx := 0x101 } mp[rx] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mp[ry] = 5; if (mp[E.B] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { E rx; assembly { rx := 0x101 } mp[E.B] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[E.B] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mp[E.B] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { E rx; assembly { rx := 0x101 } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
contract Cud_0 {
    mapping(U => uint256) mp;
    mapping(uint256 => U) mv;
    function _s_retdirect_mapping_value_key_dirty() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_key_dirty(U pv) internal   returns (uint256) { mp[pv] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function _s_retdirect_mapping_key_read() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_key_read(U pv) internal   returns (uint256) { mp[U.wrap(27)] = 9; return mp[pv]; }
    function _s_retdirect_mapping_value_raw() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mapping_value_raw(U pv) internal   returns (uint256) { mv[1] = pv; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_local_mapping_value_key_dirty() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } mp[rx] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_retdirect_mapping_value_key_dirty() external   returns (uint256) { mp[_s_retdirect_mapping_value_key_dirty()] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_param_mapping_value_key_dirty() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_key_dirty(rx); }
    function t_mem_mapping_value_key_dirty() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mp[ry] = 5; if (mp[U.wrap(27)] != 5) return 0; return 5; }
    function t_local_mapping_key_read() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } mp[U.wrap(27)] = 9; return mp[rx]; }
    function t_retdirect_mapping_key_read() external   returns (uint256) { mp[U.wrap(27)] = 9; return mp[_s_retdirect_mapping_key_read()]; }
    function t_param_mapping_key_read() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_key_read(rx); }
    function t_mem_mapping_key_read() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mp[U.wrap(27)] = 9; return mp[ry]; }
    function t_local_mapping_value_raw() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } mv[1] = rx; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_retdirect_mapping_value_raw() external   returns (uint256) { mv[1] = _s_retdirect_mapping_value_raw(); uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
    function t_param_mapping_value_raw() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mapping_value_raw(rx); }
    function t_mem_mapping_value_raw() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; mv[1] = ry; uint256 r; assembly { mstore(0, 1) mstore(32, mv.slot) r := sload(keccak256(0, 64)) } return r; }
}
