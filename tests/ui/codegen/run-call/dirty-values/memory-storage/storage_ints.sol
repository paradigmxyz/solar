//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_storage_scalar => 27
//@ run-call: Cu8_0::t_retdirect_storage_scalar => 27
//@ run-call: Cu8_0::t_param_storage_scalar => 27
//@ run-call: Cu8_0::t_mem_storage_scalar => 27
//@ run-call: Cu8_0::t_local_storage_packed => 2235153
//@ run-call: Cu8_0::t_retdirect_storage_packed => 2235153
//@ run-call: Cu8_0::t_param_storage_packed => 2235153
//@ run-call: Cu8_0::t_mem_storage_packed => 2235153
//@ run-call: Cu8_0::t_local_storage_struct_copy => 27
//@ run-call: Cu8_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cu8_0::t_param_storage_struct_copy => 27
//@ run-call: Cu8_0::t_mem_storage_struct_copy => 27
//@ run-call: Cu8_0::t_local_storage_push => 27
//@ run-call: Cu8_0::t_retdirect_storage_push => 27
//@ run-call: Cu8_0::t_param_storage_push => 27
//@ run-call: Cu8_0::t_mem_storage_push => 27
//@ run-call: Cu8_0::t_local_storage_array_elem => 27
//@ run-call: Cu8_0::t_retdirect_storage_array_elem => 27
//@ run-call: Cu8_0::t_param_storage_array_elem => 27
//@ run-call: Cu8_0::t_mem_storage_array_elem => 27
//@ run-call-fail: Cu8_0::t_local_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_retdirect_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_param_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_0::t_mem_sarray_index => 270
//@ run-call-fail: Cu8_0::t_local_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_retdirect_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_param_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_0::t_mem_sarray_index_store => 72
//@ run-call-fail: Cu8_0::t_local_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_retdirect_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu8_0::t_param_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu8_0::t_mem_sfixed_index => 73
//@ run-call: Cu64_0::t_local_storage_scalar => 27
//@ run-call: Cu64_0::t_retdirect_storage_scalar => 27
//@ run-call: Cu64_0::t_param_storage_scalar => 27
//@ run-call: Cu64_0::t_mem_storage_scalar => 27
//@ run-call: Cu64_0::t_local_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_retdirect_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_param_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_mem_storage_packed => 160560460417567937272593
//@ run-call: Cu64_0::t_local_storage_struct_copy => 27
//@ run-call: Cu64_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cu64_0::t_param_storage_struct_copy => 27
//@ run-call: Cu64_0::t_mem_storage_struct_copy => 27
//@ run-call: Cu64_0::t_local_storage_push => 27
//@ run-call: Cu64_0::t_retdirect_storage_push => 27
//@ run-call: Cu64_0::t_param_storage_push => 27
//@ run-call: Cu64_0::t_mem_storage_push => 27
//@ run-call: Cu64_0::t_local_storage_array_elem => 27
//@ run-call: Cu64_0::t_retdirect_storage_array_elem => 27
//@ run-call: Cu64_0::t_param_storage_array_elem => 27
//@ run-call: Cu64_0::t_mem_storage_array_elem => 27
//@ run-call-fail: Cu64_0::t_local_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_retdirect_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_param_sarray_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_0::t_mem_sarray_index => 270
//@ run-call-fail: Cu64_0::t_local_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_retdirect_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_param_sarray_index_store => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_0::t_mem_sarray_index_store => 72
//@ run-call-fail: Cu64_0::t_local_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_retdirect_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: Cu64_0::t_param_sfixed_index => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: Cu64_0::t_mem_sfixed_index => 73
//@ run-call: Ci8_0::t_local_storage_scalar => 253
//@ run-call: Ci8_0::t_retdirect_storage_scalar => 253
//@ run-call: Ci8_0::t_param_storage_scalar => 253
//@ run-call: Ci8_0::t_mem_storage_scalar => 253
//@ run-call: Ci8_0::t_local_storage_packed => 2293009
//@ run-call: Ci8_0::t_retdirect_storage_packed => 2293009
//@ run-call: Ci8_0::t_param_storage_packed => 2293009
//@ run-call: Ci8_0::t_mem_storage_packed => 2293009
//@ run-call: Ci8_0::t_local_storage_struct_copy => 253
//@ run-call: Ci8_0::t_retdirect_storage_struct_copy => 253
//@ run-call: Ci8_0::t_param_storage_struct_copy => 253
//@ run-call: Ci8_0::t_mem_storage_struct_copy => 253
//@ run-call: Ci8_0::t_local_storage_push => 253
//@ run-call: Ci8_0::t_retdirect_storage_push => 253
//@ run-call: Ci8_0::t_param_storage_push => 253
//@ run-call: Ci8_0::t_mem_storage_push => 253
//@ run-call: Ci8_0::t_local_storage_array_elem => 253
//@ run-call: Ci8_0::t_retdirect_storage_array_elem => 253
//@ run-call: Ci8_0::t_param_storage_array_elem => 253
//@ run-call: Ci8_0::t_mem_storage_array_elem => 253
//@ run-call: Ci24_0::t_local_storage_scalar => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_scalar => 16777213
//@ run-call: Ci24_0::t_param_storage_scalar => 16777213
//@ run-call: Ci24_0::t_mem_storage_scalar => 16777213
//@ run-call: Ci24_0::t_local_storage_packed => 150323854609
//@ run-call: Ci24_0::t_retdirect_storage_packed => 150323854609
//@ run-call: Ci24_0::t_param_storage_packed => 150323854609
//@ run-call: Ci24_0::t_mem_storage_packed => 150323854609
//@ run-call: Ci24_0::t_local_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_param_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_mem_storage_struct_copy => 16777213
//@ run-call: Ci24_0::t_local_storage_push => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_push => 16777213
//@ run-call: Ci24_0::t_param_storage_push => 16777213
//@ run-call: Ci24_0::t_mem_storage_push => 16777213
//@ run-call: Ci24_0::t_local_storage_array_elem => 16777213
//@ run-call: Ci24_0::t_retdirect_storage_array_elem => 16777213
//@ run-call: Ci24_0::t_param_storage_array_elem => 16777213
//@ run-call: Ci24_0::t_mem_storage_array_elem => 16777213

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

struct St_u8 {
    uint256 a;
    uint8 f;
}

struct St_u64 {
    uint256 a;
    uint64 f;
}

struct St_i8 {
    uint256 a;
    int8 f;
}

struct St_i24 {
    uint256 a;
    int24 f;
}

contract Cu8_0 {
    uint8 sv;
    uint256 gap;
    struct P { uint8 a; uint8 f; uint8 c; }
    P p;
    St_u8 q;
    uint8[] arr;
    uint256[] sa;
    uint256[40] sf;
    function _s_retdirect_storage_scalar() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_scalar(uint8 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_packed(uint8 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_struct_copy(uint8 pv) internal   returns (uint256) { St_u8 memory s = St_u8(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_push(uint8 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_array_elem(uint8 pv) internal   returns (uint256) { arr.push(uint8(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_sarray_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sarray_index(uint8 pv) internal   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[pv]; }
    function _s_retdirect_sarray_index_store() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sarray_index_store(uint8 pv) internal   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[pv] = 72; return sa[27]; }
    function _s_retdirect_sfixed_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_sfixed_index(uint8 pv) internal   returns (uint256) { sf[27] = 73; return sf[pv]; }
    function t_local_storage_scalar() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } St_u8 memory s = St_u8(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_u8 memory s = St_u8(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; St_u8 memory s = St_u8(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } arr.push(uint8(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(uint8(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; arr.push(uint8(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_sarray_index() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[rx]; }
    function t_retdirect_sarray_index() external   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[_s_retdirect_sarray_index()]; }
    function t_param_sarray_index() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sarray_index(rx); }
    function t_mem_sarray_index() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[ry]; }
    function t_local_sarray_index_store() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } for (uint256 i; i < 40; ++i) sa.push(0); sa[rx] = 72; return sa[27]; }
    function t_retdirect_sarray_index_store() external   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[_s_retdirect_sarray_index_store()] = 72; return sa[27]; }
    function t_param_sarray_index_store() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sarray_index_store(rx); }
    function t_mem_sarray_index_store() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(0); sa[ry] = 72; return sa[27]; }
    function t_local_sfixed_index() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } sf[27] = 73; return sf[rx]; }
    function t_retdirect_sfixed_index() external   returns (uint256) { sf[27] = 73; return sf[_s_retdirect_sfixed_index()]; }
    function t_param_sfixed_index() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_sfixed_index(rx); }
    function t_mem_sfixed_index() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; sf[27] = 73; return sf[ry]; }
}
contract Cu64_0 {
    uint64 sv;
    uint256 gap;
    struct P { uint8 a; uint64 f; uint8 c; }
    P p;
    St_u64 q;
    uint64[] arr;
    uint256[] sa;
    uint256[40] sf;
    function _s_retdirect_storage_scalar() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_scalar(uint64 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_packed(uint64 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_struct_copy(uint64 pv) internal   returns (uint256) { St_u64 memory s = St_u64(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_push(uint64 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_storage_array_elem(uint64 pv) internal   returns (uint256) { arr.push(uint64(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_sarray_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sarray_index(uint64 pv) internal   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[pv]; }
    function _s_retdirect_sarray_index_store() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sarray_index_store(uint64 pv) internal   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[pv] = 72; return sa[27]; }
    function _s_retdirect_sfixed_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_sfixed_index(uint64 pv) internal   returns (uint256) { sf[27] = 73; return sf[pv]; }
    function t_local_storage_scalar() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } St_u64 memory s = St_u64(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_u64 memory s = St_u64(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; St_u64 memory s = St_u64(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } arr.push(uint64(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(uint64(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; arr.push(uint64(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_sarray_index() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[rx]; }
    function t_retdirect_sarray_index() external   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[_s_retdirect_sarray_index()]; }
    function t_param_sarray_index() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sarray_index(rx); }
    function t_mem_sarray_index() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(i * 10); return sa[ry]; }
    function t_local_sarray_index_store() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } for (uint256 i; i < 40; ++i) sa.push(0); sa[rx] = 72; return sa[27]; }
    function t_retdirect_sarray_index_store() external   returns (uint256) { for (uint256 i; i < 40; ++i) sa.push(0); sa[_s_retdirect_sarray_index_store()] = 72; return sa[27]; }
    function t_param_sarray_index_store() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sarray_index_store(rx); }
    function t_mem_sarray_index_store() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; for (uint256 i; i < 40; ++i) sa.push(0); sa[ry] = 72; return sa[27]; }
    function t_local_sfixed_index() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } sf[27] = 73; return sf[rx]; }
    function t_retdirect_sfixed_index() external   returns (uint256) { sf[27] = 73; return sf[_s_retdirect_sfixed_index()]; }
    function t_param_sfixed_index() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_sfixed_index(rx); }
    function t_mem_sfixed_index() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; sf[27] = 73; return sf[ry]; }
}
contract Ci8_0 {
    int8 sv;
    uint256 gap;
    struct P { uint8 a; int8 f; uint8 c; }
    P p;
    St_i8 q;
    int8[] arr;
    function _s_retdirect_storage_scalar() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_scalar(int8 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_packed(int8 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_struct_copy(int8 pv) internal   returns (uint256) { St_i8 memory s = St_i8(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_push(int8 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_storage_array_elem(int8 pv) internal   returns (uint256) { arr.push(int8(-2)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } St_i8 memory s = St_i8(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_i8 memory s = St_i8(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; St_i8 memory s = St_i8(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } arr.push(int8(-2)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(int8(-2)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; arr.push(int8(-2)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
contract Ci24_0 {
    int24 sv;
    uint256 gap;
    struct P { uint8 a; int24 f; uint8 c; }
    P p;
    St_i24 q;
    int24[] arr;
    function _s_retdirect_storage_scalar() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_scalar(int24 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_packed(int24 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_struct_copy(int24 pv) internal   returns (uint256) { St_i24 memory s = St_i24(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_push(int24 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_storage_array_elem(int24 pv) internal   returns (uint256) { arr.push(int24(-2)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } St_i24 memory s = St_i24(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_i24 memory s = St_i24(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; St_i24 memory s = St_i24(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } arr.push(int24(-2)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(int24(-2)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; arr.push(int24(-2)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
