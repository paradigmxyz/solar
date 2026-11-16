//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cb1_0::t_local_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_elem => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_fixed => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_literal => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_marray_literal_dyn => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_local_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_mnested => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb4_0::t_local_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_elem => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_fixed => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_literal => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_marray_literal_dyn => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_local_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_mnested => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb20_0::t_local_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_elem => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_fixed => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_literal => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_marray_literal_dyn => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_local_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_mnested => 8234104122419153896766082834368325185836758793843649141158396535749088378880

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

contract Cb1_0 {
    function _s_retdirect_marray_elem() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_elem(bytes1 pv) internal view  returns (uint256) { bytes1[] memory a = new bytes1[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_fixed(bytes1 pv) internal view  returns (uint256) { bytes1[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_literal(bytes1 pv) internal view  returns (uint256) { bytes1[2] memory a = [pv, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_marray_literal_dyn(bytes1 pv) internal view  returns (uint256) { bytes1[] memory a = new bytes1[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_mnested(bytes1 pv) internal view  returns (uint256) { bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = pv; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory a = new bytes1[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { bytes1[] memory a = new bytes1[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory a = new bytes1[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { bytes1[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[2] memory a = [rx, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { bytes1[2] memory a = [_s_retdirect_marray_literal(), bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[2] memory a = [ry, bytes1(0xcd)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[] memory a = new bytes1[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { bytes1[] memory a = new bytes1[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[] memory a = new bytes1[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = rx; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = _s_retdirect_mnested(); bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1[][] memory a = new bytes1[][](1); a[0] = new bytes1[](1); a[0][0] = ry; bytes1[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Cb4_0 {
    function _s_retdirect_marray_elem() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_elem(bytes4 pv) internal view  returns (uint256) { bytes4[] memory a = new bytes4[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_fixed(bytes4 pv) internal view  returns (uint256) { bytes4[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_literal(bytes4 pv) internal view  returns (uint256) { bytes4[2] memory a = [pv, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_marray_literal_dyn(bytes4 pv) internal view  returns (uint256) { bytes4[] memory a = new bytes4[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_mnested(bytes4 pv) internal view  returns (uint256) { bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = pv; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory a = new bytes4[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { bytes4[] memory a = new bytes4[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory a = new bytes4[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { bytes4[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[2] memory a = [rx, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { bytes4[2] memory a = [_s_retdirect_marray_literal(), bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[2] memory a = [ry, bytes4(0x11223344)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[] memory a = new bytes4[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { bytes4[] memory a = new bytes4[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[] memory a = new bytes4[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = rx; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = _s_retdirect_mnested(); bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4[][] memory a = new bytes4[][](1); a[0] = new bytes4[](1); a[0][0] = ry; bytes4[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
contract Cb20_0 {
    function _s_retdirect_marray_elem() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_elem(bytes20 pv) internal view  returns (uint256) { bytes20[] memory a = new bytes20[](2); a[1] = pv; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function _s_retdirect_marray_fixed() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_fixed(bytes20 pv) internal view  returns (uint256) { bytes20[2] memory a; a[0] = pv; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_literal(bytes20 pv) internal view  returns (uint256) { bytes20[2] memory a = [pv, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function _s_retdirect_marray_literal_dyn() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_marray_literal_dyn(bytes20 pv) internal view  returns (uint256) { bytes20[] memory a = new bytes20[](1); a[0] = pv; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function _s_retdirect_mnested() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_mnested(bytes20 pv) internal view  returns (uint256) { bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = pv; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_local_marray_elem() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory a = new bytes20[](2); a[1] = rx; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_retdirect_marray_elem() external view  returns (uint256) { bytes20[] memory a = new bytes20[](2); a[1] = _s_retdirect_marray_elem(); uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_param_marray_elem() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_elem(rx); }
    function t_mem_marray_elem() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory a = new bytes20[](2); a[1] = ry; uint256 r; assembly { r := mload(add(a, 64)) } return r; }
    function t_local_marray_fixed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[2] memory a; a[0] = rx; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_fixed() external view  returns (uint256) { bytes20[2] memory a; a[0] = _s_retdirect_marray_fixed(); uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_fixed() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_fixed(rx); }
    function t_mem_marray_fixed() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[2] memory a; a[0] = ry; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[2] memory a = [rx, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_retdirect_marray_literal() external view  returns (uint256) { bytes20[2] memory a = [_s_retdirect_marray_literal(), bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_param_marray_literal() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_literal(rx); }
    function t_mem_marray_literal() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[2] memory a = [ry, bytes20(0x1111111111111111111111111111111111111111)]; uint256 r; assembly { r := mload(a) } return r; }
    function t_local_marray_literal_dyn() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[] memory a = new bytes20[](1); a[0] = rx; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_retdirect_marray_literal_dyn() external view  returns (uint256) { bytes20[] memory a = new bytes20[](1); a[0] = _s_retdirect_marray_literal_dyn(); uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_param_marray_literal_dyn() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_marray_literal_dyn(rx); }
    function t_mem_marray_literal_dyn() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[] memory a = new bytes20[](1); a[0] = ry; uint256 r; assembly { r := mload(add(a, 32)) } return r; }
    function t_local_mnested() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = rx; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_retdirect_mnested() external view  returns (uint256) { bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = _s_retdirect_mnested(); bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
    function t_param_mnested() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_mnested(rx); }
    function t_mem_mnested() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20[][] memory a = new bytes20[][](1); a[0] = new bytes20[](1); a[0][0] = ry; bytes20[] memory in0 = a[0]; uint256 r; assembly { r := mload(add(in0, 32)) } return r; }
}
