//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cbool_0::t_local_marray_elem => 1
//@ run-call: Cbool_0::t_retdirect_marray_elem => 1
//@ run-call: Cbool_0::t_param_marray_elem => 1
//@ run-call: Cbool_0::t_mem_marray_elem => 1
//@ run-call: Cbool_0::t_local_marray_fixed => 1
//@ run-call: Cbool_0::t_retdirect_marray_fixed => 1
//@ run-call: Cbool_0::t_param_marray_fixed => 1
//@ run-call: Cbool_0::t_mem_marray_fixed => 1
//@ run-call: Cbool_0::t_local_marray_literal => 1
//@ run-call: Cbool_0::t_retdirect_marray_literal => 1
//@ run-call: Cbool_0::t_param_marray_literal => 1
//@ run-call: Cbool_0::t_mem_marray_literal => 1
//@ run-call: Cbool_0::t_local_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_retdirect_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_param_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_mem_marray_literal_dyn => 1
//@ run-call: Cbool_0::t_local_mnested => 1
//@ run-call: Cbool_0::t_retdirect_mnested => 1
//@ run-call: Cbool_0::t_param_mnested => 1
//@ run-call: Cbool_0::t_mem_mnested => 1
//@ run-call-fail: Cen_0::t_local_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_elem => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_fixed => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_literal => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_marray_literal_dyn => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_local_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_mnested => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_marray_elem => 27
//@ run-call: Cud_0::t_retdirect_marray_elem => 27
//@ run-call: Cud_0::t_param_marray_elem => 27
//@ run-call: Cud_0::t_mem_marray_elem => 27
//@ run-call: Cud_0::t_local_marray_fixed => 27
//@ run-call: Cud_0::t_retdirect_marray_fixed => 27
//@ run-call: Cud_0::t_param_marray_fixed => 27
//@ run-call: Cud_0::t_mem_marray_fixed => 27
//@ run-call: Cud_0::t_local_marray_literal => 27
//@ run-call: Cud_0::t_retdirect_marray_literal => 27
//@ run-call: Cud_0::t_param_marray_literal => 27
//@ run-call: Cud_0::t_mem_marray_literal => 27
//@ run-call: Cud_0::t_local_marray_literal_dyn => 27
//@ run-call: Cud_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cud_0::t_param_marray_literal_dyn => 27
//@ run-call: Cud_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cud_0::t_local_mnested => 27
//@ run-call: Cud_0::t_retdirect_mnested => 27
//@ run-call: Cud_0::t_param_mnested => 27
//@ run-call: Cud_0::t_mem_mnested => 27

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

contract Cbool_0 {
    function _s_retdirect_marray_elem() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_elem(bool pv) internal view  returns (uint256) { bool[] memory a = new bool[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_fixed(bool pv) internal view  returns (uint256) { bool[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_literal(bool pv) internal view  returns (uint256) { bool[2] memory a = [pv, false]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_marray_literal_dyn(bool pv) internal view  returns (uint256) { bool[] memory a = new bool[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_mnested(bool pv) internal view  returns (uint256) { bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = pv; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory a = new bool[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { bool[] memory a = new bool[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory a = new bool[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { bool[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[2] memory a = [rx, false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { bool[2] memory a = [_s_retdirect_marray_literal(), false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[2] memory a = [ry, false]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[] memory a = new bool[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { bool[] memory a = new bool[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[] memory a = new bool[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = rx; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = _s_retdirect_mnested(); bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool[][] memory a = new bool[][](1); a[0] = new bool[](1); a[0][0] = ry; bool[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Cen_0 {
    function _s_retdirect_marray_elem() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_elem(E pv) internal view  returns (uint256) { E[] memory a = new E[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_fixed(E pv) internal view  returns (uint256) { E[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_literal(E pv) internal view  returns (uint256) { E[2] memory a = [pv, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_marray_literal_dyn(E pv) internal view  returns (uint256) { E[] memory a = new E[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_mnested(E pv) internal view  returns (uint256) { E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = pv; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory a = new E[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { E[] memory a = new E[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory a = new E[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { E[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[2] memory a = [rx, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { E[2] memory a = [_s_retdirect_marray_literal(), E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[2] memory a = [ry, E.A]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[] memory a = new E[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { E[] memory a = new E[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[] memory a = new E[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = rx; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = _s_retdirect_mnested(); E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E[][] memory a = new E[][](1); a[0] = new E[](1); a[0][0] = ry; E[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Cud_0 {
    function _s_retdirect_marray_elem() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_elem(U pv) internal view  returns (uint256) { U[] memory a = new U[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_fixed(U pv) internal view  returns (uint256) { U[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal(U pv) internal view  returns (uint256) { U[2] memory a = [pv, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal_dyn(U pv) internal view  returns (uint256) { U[] memory a = new U[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_mnested(U pv) internal view  returns (uint256) { U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = pv; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory a = new U[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { U[] memory a = new U[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory a = new U[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { U[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[2] memory a = [rx, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { U[2] memory a = [_s_retdirect_marray_literal(), U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[2] memory a = [ry, U.wrap(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[] memory a = new U[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { U[] memory a = new U[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[] memory a = new U[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = rx; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = _s_retdirect_mnested(); U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U[][] memory a = new U[][](1); a[0] = new U[](1); a[0][0] = ry; U[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
