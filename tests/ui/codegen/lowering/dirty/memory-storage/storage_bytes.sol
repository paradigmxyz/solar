//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_storage_scalar => 171
//@ run-call: Cb1_0::t_retdirect_storage_scalar => 171
//@ run-call: Cb1_0::t_param_storage_scalar => 171
//@ run-call: Cb1_0::t_mem_storage_scalar => 171
//@ run-call: Cb1_0::t_local_storage_packed => 2272017
//@ run-call: Cb1_0::t_retdirect_storage_packed => 2272017
//@ run-call: Cb1_0::t_param_storage_packed => 2272017
//@ run-call: Cb1_0::t_mem_storage_packed => 2272017
//@ run-call: Cb1_0::t_local_storage_struct_copy => 171
//@ run-call: Cb1_0::t_retdirect_storage_struct_copy => 171
//@ run-call: Cb1_0::t_param_storage_struct_copy => 171
//@ run-call: Cb1_0::t_mem_storage_struct_copy => 171
//@ run-call: Cb1_0::t_local_storage_push => 171
//@ run-call: Cb1_0::t_retdirect_storage_push => 171
//@ run-call: Cb1_0::t_param_storage_push => 171
//@ run-call: Cb1_0::t_mem_storage_push => 171
//@ run-call: Cb1_0::t_local_storage_array_elem => 171
//@ run-call: Cb1_0::t_retdirect_storage_array_elem => 171
//@ run-call: Cb1_0::t_param_storage_array_elem => 171
//@ run-call: Cb1_0::t_mem_storage_array_elem => 171
//@ run-call: Cb1_0::t_local_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_0::t_retdirect_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_0::t_param_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_0::t_mem_storage_bytes_push => 77345497107738552411838431392522000948863935069627095510731433067785723314178
//@ run-call: Cb1_0::t_local_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_0::t_retdirect_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_0::t_param_bytes_to_storage_bytesN => 171
//@ run-call: Cb1_0::t_mem_bytes_to_storage_bytesN => 171
//@ run-call: Cb4_0::t_local_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_param_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_mem_storage_scalar => 3735928559
//@ run-call: Cb4_0::t_local_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_retdirect_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_param_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_mem_storage_packed => 38339793055505
//@ run-call: Cb4_0::t_local_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_param_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_mem_storage_struct_copy => 3735928559
//@ run-call: Cb4_0::t_local_storage_push => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_push => 3735928559
//@ run-call: Cb4_0::t_param_storage_push => 3735928559
//@ run-call: Cb4_0::t_mem_storage_push => 3735928559
//@ run-call: Cb4_0::t_local_storage_array_elem => 3735928559
//@ run-call: Cb4_0::t_retdirect_storage_array_elem => 3735928559
//@ run-call: Cb4_0::t_param_storage_array_elem => 3735928559
//@ run-call: Cb4_0::t_mem_storage_array_elem => 3735928559
//@ run-call: Cb4_0::t_local_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_0::t_retdirect_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_0::t_param_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb4_0::t_mem_bytes_to_storage_bytesN => 3735928559
//@ run-call: Cb20_0::t_local_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_scalar => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_retdirect_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_param_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_mem_storage_packed => 12747516076686804336366434108400172844360657175875601
//@ run-call: Cb20_0::t_local_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_struct_copy => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_push => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_storage_array_elem => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_local_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_retdirect_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_param_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080
//@ run-call: Cb20_0::t_mem_bytes_to_storage_bytesN => 103929005307130220006098923584552504982110632080

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

struct St_b1 {
    uint256 a;
    bytes1 f;
}

struct St_b4 {
    uint256 a;
    bytes4 f;
}

struct St_b20 {
    uint256 a;
    bytes20 f;
}

contract Cb1_0 {
    bytes1 sv;
    uint256 gap;
    struct P { uint8 a; bytes1 f; uint8 c; }
    P p;
    St_b1 q;
    bytes1[] arr;
    bytes bs;
    bytes1 sb;
    function _s_retdirect_storage_scalar() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_scalar(bytes1 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_packed(bytes1 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_struct_copy(bytes1 pv) internal   returns (uint256) { St_b1 memory s = St_b1(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_push(bytes1 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_array_elem(bytes1 pv) internal   returns (uint256) { arr.push(bytes1(0xcd)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_bytes_push() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_storage_bytes_push(bytes1 pv) internal   returns (uint256) { bs.push(pv); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_bytes_to_storage_bytesN(bytes1 pv) internal   returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } St_b1 memory s = St_b1(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_b1 memory s = St_b1(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; St_b1 memory s = St_b1(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } arr.push(bytes1(0xcd)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(bytes1(0xcd)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; arr.push(bytes1(0xcd)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_bytes_push() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bs.push(rx); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_retdirect_storage_bytes_push() external   returns (uint256) { bs.push(_s_retdirect_storage_bytes_push()); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_param_storage_bytes_push() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_storage_bytes_push(rx); }
    function t_mem_storage_bytes_push() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bs.push(ry); uint256 r; assembly { r := sload(bs.slot) } return r; }
    function t_local_bytes_to_storage_bytesN() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external   returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
contract Cb4_0 {
    bytes4 sv;
    uint256 gap;
    struct P { uint8 a; bytes4 f; uint8 c; }
    P p;
    St_b4 q;
    bytes4[] arr;
    bytes4 sb;
    function _s_retdirect_storage_scalar() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_scalar(bytes4 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_packed(bytes4 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_struct_copy(bytes4 pv) internal   returns (uint256) { St_b4 memory s = St_b4(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_push(bytes4 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_storage_array_elem(bytes4 pv) internal   returns (uint256) { arr.push(bytes4(0x11223344)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_bytes_to_storage_bytesN(bytes4 pv) internal   returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } St_b4 memory s = St_b4(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_b4 memory s = St_b4(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; St_b4 memory s = St_b4(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } arr.push(bytes4(0x11223344)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(bytes4(0x11223344)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; arr.push(bytes4(0x11223344)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_bytes_to_storage_bytesN() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external   returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
contract Cb20_0 {
    bytes20 sv;
    uint256 gap;
    struct P { uint8 a; bytes20 f; uint8 c; }
    P p;
    St_b20 q;
    bytes20[] arr;
    bytes20 sb;
    function _s_retdirect_storage_scalar() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_scalar(bytes20 pv) internal   returns (uint256) { sv = pv; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function _s_retdirect_storage_packed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_packed(bytes20 pv) internal   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = pv; uint256 r; assembly { r := sload(p.slot) } return r; }
    function _s_retdirect_storage_struct_copy() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_struct_copy(bytes20 pv) internal   returns (uint256) { St_b20 memory s = St_b20(1, pv); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function _s_retdirect_storage_push() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_push(bytes20 pv) internal   returns (uint256) { arr.push(pv); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_storage_array_elem() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_storage_array_elem(bytes20 pv) internal   returns (uint256) { arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = pv; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function _s_retdirect_bytes_to_storage_bytesN() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_bytes_to_storage_bytesN(bytes20 pv) internal   returns (uint256) { sb = pv; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_local_storage_scalar() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } sv = rx; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_retdirect_storage_scalar() external   returns (uint256) { sv = _s_retdirect_storage_scalar(); uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_param_storage_scalar() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_scalar(rx); }
    function t_mem_storage_scalar() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; sv = ry; uint256 r; assembly { r := sload(sv.slot) } return r; }
    function t_local_storage_packed() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } p.a = 0x11; p.c = 0x22; p.f = rx; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_retdirect_storage_packed() external   returns (uint256) { p.a = 0x11; p.c = 0x22; p.f = _s_retdirect_storage_packed(); uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_param_storage_packed() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_packed(rx); }
    function t_mem_storage_packed() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; p.a = 0x11; p.c = 0x22; p.f = ry; uint256 r; assembly { r := sload(p.slot) } return r; }
    function t_local_storage_struct_copy() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } St_b20 memory s = St_b20(1, rx); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_retdirect_storage_struct_copy() external   returns (uint256) { St_b20 memory s = St_b20(1, _s_retdirect_storage_struct_copy()); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_param_storage_struct_copy() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_struct_copy(rx); }
    function t_mem_storage_struct_copy() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; St_b20 memory s = St_b20(1, ry); q = s; uint256 r; assembly { r := sload(add(q.slot, 1)) } return r; }
    function t_local_storage_push() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } arr.push(rx); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_push() external   returns (uint256) { arr.push(_s_retdirect_storage_push()); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_push() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_push(rx); }
    function t_mem_storage_push() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; arr.push(ry); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_storage_array_elem() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = rx; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_retdirect_storage_array_elem() external   returns (uint256) { arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = _s_retdirect_storage_array_elem(); uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_param_storage_array_elem() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_storage_array_elem(rx); }
    function t_mem_storage_array_elem() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; arr.push(bytes20(0x1111111111111111111111111111111111111111)); arr[0] = ry; uint256 r; assembly { mstore(0, arr.slot) r := sload(keccak256(0, 32)) } return r; }
    function t_local_bytes_to_storage_bytesN() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } sb = rx; uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_retdirect_bytes_to_storage_bytesN() external   returns (uint256) { sb = _s_retdirect_bytes_to_storage_bytesN(); uint256 r; assembly { r := sload(sb.slot) } return r; }
    function t_param_bytes_to_storage_bytesN() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_bytes_to_storage_bytesN(rx); }
    function t_mem_bytes_to_storage_bytesN() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; sb = ry; uint256 r; assembly { r := sload(sb.slot) } return r; }
}
