//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_marray_elem => 27
//@ run-call: Cu8_0::t_retdirect_marray_elem => 27
//@ run-call: Cu8_0::t_param_marray_elem => 27
//@ run-call: Cu8_0::t_mem_marray_elem => 27
//@ run-call: Cu8_0::t_local_marray_fixed => 27
//@ run-call: Cu8_0::t_retdirect_marray_fixed => 27
//@ run-call: Cu8_0::t_param_marray_fixed => 27
//@ run-call: Cu8_0::t_mem_marray_fixed => 27
//@ run-call: Cu8_0::t_local_marray_literal => 27
//@ run-call: Cu8_0::t_retdirect_marray_literal => 27
//@ run-call: Cu8_0::t_param_marray_literal => 27
//@ run-call: Cu8_0::t_mem_marray_literal => 27
//@ run-call: Cu8_0::t_local_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_param_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cu8_0::t_local_mnested => 27
//@ run-call: Cu8_0::t_retdirect_mnested => 27
//@ run-call: Cu8_0::t_param_mnested => 27
//@ run-call: Cu8_0::t_mem_mnested => 27
//@ run-call: Cu8_0::t_local_marray_index => 70
//@ run-call: Cu8_0::t_retdirect_marray_index => 70
//@ run-call: Cu8_0::t_param_marray_index => 70
//@ run-call: Cu8_0::t_mem_marray_index => 70
//@ run-call: Cu8_0::t_local_marray_index_store => 71
//@ run-call: Cu8_0::t_retdirect_marray_index_store => 71
//@ run-call: Cu8_0::t_param_marray_index_store => 71
//@ run-call: Cu8_0::t_mem_marray_index_store => 71
//@ run-call: Cu64_0::t_local_marray_elem => 27
//@ run-call: Cu64_0::t_retdirect_marray_elem => 27
//@ run-call: Cu64_0::t_param_marray_elem => 27
//@ run-call: Cu64_0::t_mem_marray_elem => 27
//@ run-call: Cu64_0::t_local_marray_fixed => 27
//@ run-call: Cu64_0::t_retdirect_marray_fixed => 27
//@ run-call: Cu64_0::t_param_marray_fixed => 27
//@ run-call: Cu64_0::t_mem_marray_fixed => 27
//@ run-call: Cu64_0::t_local_marray_literal => 27
//@ run-call: Cu64_0::t_retdirect_marray_literal => 27
//@ run-call: Cu64_0::t_param_marray_literal => 27
//@ run-call: Cu64_0::t_mem_marray_literal => 27
//@ run-call: Cu64_0::t_local_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_retdirect_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_param_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_mem_marray_literal_dyn => 27
//@ run-call: Cu64_0::t_local_mnested => 27
//@ run-call: Cu64_0::t_retdirect_mnested => 27
//@ run-call: Cu64_0::t_param_mnested => 27
//@ run-call: Cu64_0::t_mem_mnested => 27
//@ run-call: Cu64_0::t_local_marray_index => 70
//@ run-call: Cu64_0::t_retdirect_marray_index => 70
//@ run-call: Cu64_0::t_param_marray_index => 70
//@ run-call: Cu64_0::t_mem_marray_index => 70
//@ run-call: Cu64_0::t_local_marray_index_store => 71
//@ run-call: Cu64_0::t_retdirect_marray_index_store => 71
//@ run-call: Cu64_0::t_param_marray_index_store => 71
//@ run-call: Cu64_0::t_mem_marray_index_store => 71
//@ run-call: Ci8_0::t_local_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_local_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_elem => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_fixed => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_literal => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_marray_literal_dyn => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_mnested => 115792089237316195423570985008687907853269984665640564039457584007913129639933

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
    function _s_retdirect_marray_elem() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_elem(uint8 pv) internal view  returns (uint256) { uint8[] memory a = new uint8[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_fixed(uint8 pv) internal view  returns (uint256) { uint8[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal(uint8 pv) internal view  returns (uint256) { uint8[2] memory a = [pv, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_literal_dyn(uint8 pv) internal view  returns (uint256) { uint8[] memory a = new uint8[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_mnested(uint8 pv) internal view  returns (uint256) { uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = pv; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_marray_index() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_index(uint8 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[pv]; }
    function _s_retdirect_marray_index_store() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_marray_index_store(uint8 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](40); a[pv] = 71; return a[27]; }
    function t_local_marray_elem() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory a = new uint8[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { uint8[] memory a = new uint8[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory a = new uint8[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { uint8[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[2] memory a = [rx, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { uint8[2] memory a = [_s_retdirect_marray_literal(), uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[2] memory a = [ry, uint8(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[] memory a = new uint8[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { uint8[] memory a = new uint8[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[] memory a = new uint8[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = rx; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = _s_retdirect_mnested(); uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8[][] memory a = new uint8[][](1); a[0] = new uint8[](1); a[0][0] = ry; uint8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](40); a[27] = 70; return a[rx]; }
    function t_retdirect_marray_index() external view  returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[_s_retdirect_marray_index()]; }
    function t_param_marray_index() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_index(rx); }
    function t_mem_marray_index() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](40); a[27] = 70; return a[ry]; }
    function t_local_marray_index_store() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint256[] memory a = new uint256[](40); a[rx] = 71; return a[27]; }
    function t_retdirect_marray_index_store() external view  returns (uint256) { uint256[] memory a = new uint256[](40); a[_s_retdirect_marray_index_store()] = 71; return a[27]; }
    function t_param_marray_index_store() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_marray_index_store(rx); }
    function t_mem_marray_index_store() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint256[] memory a = new uint256[](40); a[ry] = 71; return a[27]; }
}
contract Cu64_0 {
    function _s_retdirect_marray_elem() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_elem(uint64 pv) internal view  returns (uint256) { uint64[] memory a = new uint64[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_fixed(uint64 pv) internal view  returns (uint256) { uint64[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_literal(uint64 pv) internal view  returns (uint256) { uint64[2] memory a = [pv, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_literal_dyn(uint64 pv) internal view  returns (uint256) { uint64[] memory a = new uint64[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_mnested(uint64 pv) internal view  returns (uint256) { uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = pv; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function _s_retdirect_marray_index() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_index(uint64 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[pv]; }
    function _s_retdirect_marray_index_store() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_marray_index_store(uint64 pv) internal view  returns (uint256) { uint256[] memory a = new uint256[](40); a[pv] = 71; return a[27]; }
    function t_local_marray_elem() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory a = new uint64[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { uint64[] memory a = new uint64[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory a = new uint64[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { uint64[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[2] memory a = [rx, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { uint64[2] memory a = [_s_retdirect_marray_literal(), uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[2] memory a = [ry, uint64(3)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[] memory a = new uint64[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { uint64[] memory a = new uint64[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[] memory a = new uint64[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = rx; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = _s_retdirect_mnested(); uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64[][] memory a = new uint64[][](1); a[0] = new uint64[](1); a[0][0] = ry; uint64[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](40); a[27] = 70; return a[rx]; }
    function t_retdirect_marray_index() external view  returns (uint256) { uint256[] memory a = new uint256[](40); a[27] = 70; return a[_s_retdirect_marray_index()]; }
    function t_param_marray_index() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_index(rx); }
    function t_mem_marray_index() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](40); a[27] = 70; return a[ry]; }
    function t_local_marray_index_store() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint256[] memory a = new uint256[](40); a[rx] = 71; return a[27]; }
    function t_retdirect_marray_index_store() external view  returns (uint256) { uint256[] memory a = new uint256[](40); a[_s_retdirect_marray_index_store()] = 71; return a[27]; }
    function t_param_marray_index_store() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_marray_index_store(rx); }
    function t_mem_marray_index_store() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint256[] memory a = new uint256[](40); a[ry] = 71; return a[27]; }
}
contract Ci8_0 {
    function _s_retdirect_marray_elem() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_elem(int8 pv) internal view  returns (uint256) { int8[] memory a = new int8[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_fixed(int8 pv) internal view  returns (uint256) { int8[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_literal(int8 pv) internal view  returns (uint256) { int8[2] memory a = [pv, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_marray_literal_dyn(int8 pv) internal view  returns (uint256) { int8[] memory a = new int8[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_mnested(int8 pv) internal view  returns (uint256) { int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = pv; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory a = new int8[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { int8[] memory a = new int8[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory a = new int8[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { int8[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[2] memory a = [rx, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { int8[2] memory a = [_s_retdirect_marray_literal(), int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[2] memory a = [ry, int8(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[] memory a = new int8[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { int8[] memory a = new int8[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[] memory a = new int8[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = rx; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = _s_retdirect_mnested(); int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8[][] memory a = new int8[][](1); a[0] = new int8[](1); a[0][0] = ry; int8[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Ci24_0 {
    function _s_retdirect_marray_elem() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_elem(int24 pv) internal view  returns (uint256) { int24[] memory a = new int24[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_fixed(int24 pv) internal view  returns (uint256) { int24[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_literal(int24 pv) internal view  returns (uint256) { int24[2] memory a = [pv, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_marray_literal_dyn(int24 pv) internal view  returns (uint256) { int24[] memory a = new int24[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_mnested(int24 pv) internal view  returns (uint256) { int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = pv; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory a = new int24[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { int24[] memory a = new int24[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory a = new int24[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { int24[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[2] memory a = [rx, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { int24[2] memory a = [_s_retdirect_marray_literal(), int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[2] memory a = [ry, int24(-2)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[] memory a = new int24[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { int24[] memory a = new int24[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[] memory a = new int24[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = rx; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = _s_retdirect_mnested(); int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24[][] memory a = new int24[][](1); a[0] = new int24[](1); a[0][0] = ry; int24[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
