//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Caddr_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddr_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Caddrp_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cctr_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080

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

struct St_addr {
    uint256 a;
    address f;
}

struct St_addrp {
    uint256 a;
    address payable f;
}

struct St_ctr {
    uint256 a;
    IC f;
}

contract Caddr_0 {
    address sv;
    uint256 gap;
    struct P { uint8 a; address f; uint8 c; }
    P p;
    St_addr q;
    address[] arr;
    function _s_retdirect_storage_scalar() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(address pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(address pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(address pv) internal   returns (uint256) { St_addr memory s = St_addr(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(address pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(address pv) internal   returns (uint256) { arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addr memory s = St_addr(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_addr memory s = St_addr(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; St_addr memory s = St_addr(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; arr.push(address(0x1111111111111111111111111111111111111111)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
contract Caddrp_0 {
    address payable sv;
    uint256 gap;
    struct P { uint8 a; address payable f; uint8 c; }
    P p;
    St_addrp q;
    address payable[] arr;
    function _s_retdirect_storage_scalar() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(address payable pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(address payable pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(address payable pv) internal   returns (uint256) { St_addrp memory s = St_addrp(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(address payable pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(address payable pv) internal   returns (uint256) { arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_addrp memory s = St_addrp(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_addrp memory s = St_addrp(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; St_addrp memory s = St_addrp(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; arr.push(payable(address(0x1111111111111111111111111111111111111111))); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
contract Cctr_0 {
    IC sv;
    uint256 gap;
    struct P { uint8 a; IC f; uint8 c; }
    P p;
    St_ctr q;
    IC[] arr;
    function _s_retdirect_storage_scalar() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_scalar(IC pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_packed(IC pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_struct_copy(IC pv) internal   returns (uint256) { St_ctr memory s = St_ctr(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_push(IC pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_storage_array_elem(IC pv) internal   returns (uint256) { arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } St_ctr memory s = St_ctr(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_ctr memory s = St_ctr(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; St_ctr memory s = St_ctr(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; arr.push(IC(address(0x1111111111111111111111111111111111111111))); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
}
