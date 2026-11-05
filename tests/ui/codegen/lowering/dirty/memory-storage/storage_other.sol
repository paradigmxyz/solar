//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_storage_scalar => 1
//@ run-call: Cbool_0::t_retdirect_storage_scalar => 1
//@ run-call: Cbool_0::t_param_storage_scalar => 1
//@ run-call: Cbool_0::t_mem_storage_scalar => 1
//@ run-call: Cbool_0::t_local_storage_packed => 2228497
//@ run-call: Cbool_0::t_retdirect_storage_packed => 2228497
//@ run-call: Cbool_0::t_param_storage_packed => 2228497
//@ run-call: Cbool_0::t_mem_storage_packed => 2228497
//@ run-call: Cbool_0::t_local_storage_struct_copy => 1
//@ run-call: Cbool_0::t_retdirect_storage_struct_copy => 1
//@ run-call: Cbool_0::t_param_storage_struct_copy => 1
//@ run-call: Cbool_0::t_mem_storage_struct_copy => 1
//@ run-call: Cbool_0::t_local_storage_push => 1
//@ run-call: Cbool_0::t_retdirect_storage_push => 1
//@ run-call: Cbool_0::t_param_storage_push => 1
//@ run-call: Cbool_0::t_mem_storage_push => 1
//@ run-call: Cbool_0::t_local_storage_array_elem => 1
//@ run-call: Cbool_0::t_retdirect_storage_array_elem => 1
//@ run-call: Cbool_0::t_param_storage_array_elem => 1
//@ run-call: Cbool_0::t_mem_storage_array_elem => 1
//@ run-call-fail: Cen_0::t_local_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_scalar => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_packed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_struct_copy => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_push => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_storage_array_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_storage_scalar => 27
//@ run-call: Cud_0::t_retdirect_storage_scalar => 27
//@ run-call: Cud_0::t_param_storage_scalar => 27
//@ run-call: Cud_0::t_mem_storage_scalar => 27
//@ run-call: Cud_0::t_local_storage_packed => 2235153
//@ run-call: Cud_0::t_retdirect_storage_packed => 2235153
//@ run-call: Cud_0::t_param_storage_packed => 2235153
//@ run-call: Cud_0::t_mem_storage_packed => 2235153
//@ run-call: Cud_0::t_local_storage_struct_copy => 27
//@ run-call: Cud_0::t_retdirect_storage_struct_copy => 27
//@ run-call: Cud_0::t_param_storage_struct_copy => 27
//@ run-call: Cud_0::t_mem_storage_struct_copy => 27
//@ run-call: Cud_0::t_local_storage_push => 27
//@ run-call: Cud_0::t_retdirect_storage_push => 27
//@ run-call: Cud_0::t_param_storage_push => 27
//@ run-call: Cud_0::t_mem_storage_push => 27
//@ run-call: Cud_0::t_local_storage_array_elem => 27
//@ run-call: Cud_0::t_retdirect_storage_array_elem => 27
//@ run-call: Cud_0::t_param_storage_array_elem => 27
//@ run-call: Cud_0::t_mem_storage_array_elem => 27

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

struct St_bool {
    uint256 a;
    bool f;
}

struct St_en {
    uint256 a;
    E f;
}

struct St_ud {
    uint256 a;
    U f;
}

contract Cbool_0 {
    bool sv;
    uint256 gap;
    struct P { uint8 a; bool f; uint8 c; }
    P p;
    St_bool q;
    bool[] arr;
    function _s_retdirect_storage_scalar() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_scalar(bool pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_packed(bool pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_struct_copy(bool pv) internal   returns (uint256) { St_bool memory s = St_bool(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_push(bool pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_storage_array_elem(bool pv) internal   returns (uint256) { arr.push(false); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { bool rx; assembly { rx := 0x100 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { bool rx; assembly { rx := 0x100 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { bool rx; assembly { rx := 0x100 } St_bool memory s = St_bool(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_bool memory s = St_bool(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; St_bool memory s = St_bool(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { bool rx; assembly { rx := 0x100 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { bool rx; assembly { rx := 0x100 } arr.push(false); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(false); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; arr.push(false); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
contract Cen_0 {
    E sv;
    uint256 gap;
    struct P { uint8 a; E f; uint8 c; }
    P p;
    St_en q;
    E[] arr;
    function _s_retdirect_storage_scalar() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_scalar(E pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_packed(E pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_struct_copy(E pv) internal   returns (uint256) { St_en memory s = St_en(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_push(E pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_storage_array_elem(E pv) internal   returns (uint256) { arr.push(E.A); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { E rx; assembly { rx := 0x101 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { E rx; assembly { rx := 0x101 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { E rx; assembly { rx := 0x101 } St_en memory s = St_en(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_en memory s = St_en(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; St_en memory s = St_en(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { E rx; assembly { rx := 0x101 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { E rx; assembly { rx := 0x101 } arr.push(E.A); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(E.A); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; arr.push(E.A); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
contract Cud_0 {
    U sv;
    uint256 gap;
    struct P { uint8 a; U f; uint8 c; }
    P p;
    St_ud q;
    U[] arr;
    function _s_retdirect_storage_scalar() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_scalar(U pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_packed(U pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_struct_copy(U pv) internal   returns (uint256) { St_ud memory s = St_ud(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_push(U pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_storage_array_elem(U pv) internal   returns (uint256) { arr.push(U.wrap(3)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } St_ud memory s = St_ud(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_ud memory s = St_ud(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; St_ud memory s = St_ud(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } arr.push(U.wrap(3)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(U.wrap(3)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; arr.push(U.wrap(3)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
