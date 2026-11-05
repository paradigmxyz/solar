//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_retdirect_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_ret_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_param_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_copy_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_ternary_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_ifjoin_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_loopjoin_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_tuple_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_modifier_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_ptr_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_mem_unc_all => 70962722674962476623647184026178809903069531043083173323156624029881013036422
//@ run-call: Cu8_0::t_local_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_retdirect_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_ret_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_param_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_copy_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_ternary_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_ifjoin_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_loopjoin_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_tuple_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_modifier_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_ptr_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_mem_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu8_0::t_local_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_retdirect_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_ret_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_param_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_copy_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_ternary_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_ifjoin_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_loopjoin_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_tuple_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_modifier_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_ptr_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu8_0::t_mem_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_local_unc_all => 75579457905198615486476417240338682702547637704108518948688979083488157683523
//@ run-call: Cu64_0::t_retdirect_unc_all => 75579457905198615486476417240338682702547637704108518948688979083488157683523
//@ run-call: Cu64_0::t_param_unc_all => 75579457905198615486476417240338682702547637704108518948688979083488157683523
//@ run-call: Cu64_0::t_mem_unc_all => 75579457905198615486476417240338682702547637704108518948688979083488157683523
//@ run-call: Cu64_0::t_local_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu64_0::t_retdirect_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu64_0::t_param_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu64_0::t_mem_divmod_all => 11335172529576598175163996694267682434355760952122693129392089412037000014798
//@ run-call: Cu64_0::t_local_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_retdirect_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_param_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Cu64_0::t_mem_incdec_unc => 4207913267966183785708297255441997706991693557351464516577381270104168538283
//@ run-call: Ci8_0::t_local_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci8_0::t_retdirect_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci8_0::t_param_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci8_0::t_mem_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci8_0::t_local_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci8_0::t_retdirect_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci8_0::t_param_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci8_0::t_mem_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci8_0::t_local_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_retdirect_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_param_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci8_0::t_mem_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_local_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci24_0::t_retdirect_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci24_0::t_param_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci24_0::t_mem_unc_all => 51378745279449804963541915041720369589683578420764789001629917382320868443037
//@ run-call: Ci24_0::t_local_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci24_0::t_retdirect_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci24_0::t_param_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci24_0::t_mem_divmod_all => 23887086056848759132248851669647421211191716639286172303577697888898733855605
//@ run-call: Ci24_0::t_local_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_retdirect_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_param_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099
//@ run-call: Ci24_0::t_mem_incdec_unc => 102934036042693112637004719121940436604971208646931829252833655110015987537099

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: operators on dirty operands.

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
    function _s_retdirect_unc_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_unc_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_unc_all(uint8 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv + uint8(3), pv - uint8(3), pv * uint8(3), uint8(3) + pv, uint8(3) - pv, uint8(3) * pv))); } }
    uint256 _res_modifier_unc_all;
    modifier _m_modifier_unc_all(uint8 pv) { unchecked { _res_modifier_unc_all = uint256(keccak256(abi.encode(pv + uint8(3), pv - uint8(3), pv * uint8(3), uint8(3) + pv, uint8(3) - pv, uint8(3) * pv))); } _; }
    function _e_modifier_unc_all(uint8 rd) internal _m_modifier_unc_all(rd)  returns (uint256) { return _res_modifier_unc_all; }
    function _w_ptr_unc_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_divmod_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_divmod_all() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_divmod_all(uint8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv / uint8(3), pv % uint8(3), uint8(3) / pv, uint8(3) % pv))); }
    uint256 _res_modifier_divmod_all;
    modifier _m_modifier_divmod_all(uint8 pv) { _res_modifier_divmod_all = uint256(keccak256(abi.encode(pv / uint8(3), pv % uint8(3), uint8(3) / pv, uint8(3) % pv))); _; }
    function _e_modifier_divmod_all(uint8 rd) internal _m_modifier_divmod_all(rd)  returns (uint256) { return _res_modifier_divmod_all; }
    function _w_ptr_divmod_all() internal pure returns (uint256) { return 0xabcd1b; }
    function _s_retdirect_incdec_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_incdec_unc() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_incdec_unc(uint8 pv) internal view  returns (uint256) { unchecked { uint8 z1 = pv; z1++; uint8 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    uint256 _res_modifier_incdec_unc;
    modifier _m_modifier_incdec_unc(uint8 pv) { unchecked { uint8 z1 = pv; z1++; uint8 z2 = pv; --z2; _res_modifier_incdec_unc = uint256(keccak256(abi.encode(z1, z2))); } _; }
    function _e_modifier_incdec_unc(uint8 rd) internal _m_modifier_incdec_unc(rd)  returns (uint256) { return _res_modifier_incdec_unc; }
    function _w_ptr_incdec_unc() internal pure returns (uint256) { return 0xabcd1b; }
    function t_local_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx + uint8(3), rx - uint8(3), rx * uint8(3), uint8(3) + rx, uint8(3) - rx, uint8(3) * rx))); } }
    function t_retdirect_unc_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_unc_all() + uint8(3), _s_retdirect_unc_all() - uint8(3), _s_retdirect_unc_all() * uint8(3), uint8(3) + _s_retdirect_unc_all(), uint8(3) - _s_retdirect_unc_all(), uint8(3) * _s_retdirect_unc_all()))); } }
    function t_ret_unc_all() external view  returns (uint256) { unchecked { uint8 rx = _s_ret_unc_all(); return uint256(keccak256(abi.encode(rx + uint8(3), rx - uint8(3), rx * uint8(3), uint8(3) + rx, uint8(3) - rx, uint8(3) * rx))); } }
    function t_param_unc_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_unc_all(rx); }
    function t_copy_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_ternary_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_ifjoin_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_loopjoin_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_tuple_unc_all() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_modifier_unc_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_unc_all(rx); }
    function t_ptr_unc_all() external view  returns (uint256) { unchecked { function() internal pure returns (uint256) rp = _w_ptr_unc_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_mem_unc_all() external view  returns (uint256) { unchecked { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(ry + uint8(3), ry - uint8(3), ry * uint8(3), uint8(3) + ry, uint8(3) - ry, uint8(3) * ry))); } }
    function t_local_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return uint256(keccak256(abi.encode(rx / uint8(3), rx % uint8(3), uint8(3) / rx, uint8(3) % rx))); }
    function t_retdirect_divmod_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_divmod_all() / uint8(3), _s_retdirect_divmod_all() % uint8(3), uint8(3) / _s_retdirect_divmod_all(), uint8(3) % _s_retdirect_divmod_all()))); }
    function t_ret_divmod_all() external view  returns (uint256) { uint8 rx = _s_ret_divmod_all(); return uint256(keccak256(abi.encode(rx / uint8(3), rx % uint8(3), uint8(3) / rx, uint8(3) % rx))); }
    function t_param_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_divmod_all(rx); }
    function t_copy_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_ternary_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_ifjoin_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_loopjoin_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_tuple_divmod_all() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_modifier_divmod_all() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_divmod_all(rx); }
    function t_ptr_divmod_all() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_divmod_all; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_mem_divmod_all() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; return uint256(keccak256(abi.encode(ry / uint8(3), ry % uint8(3), uint8(3) / ry, uint8(3) % ry))); }
    function t_local_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 z1 = rx; z1++; uint8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_retdirect_incdec_unc() external view  returns (uint256) { unchecked { uint8 z1 = _s_retdirect_incdec_unc(); z1++; uint8 z2 = _s_retdirect_incdec_unc(); --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_ret_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx = _s_ret_incdec_unc(); uint8 z1 = rx; z1++; uint8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_param_incdec_unc() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_incdec_unc(rx); }
    function t_copy_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_ternary_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry = rf ? rx : ro; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_ifjoin_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } bool rf = gasleft() > 0; uint8 ro = uint8(3); uint8 ry; if (rf) { ry = rx; } else { ry = ro; } uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_loopjoin_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = uint8(3); for (uint256 ri; ri < 1; ++ri) { ry = rx; } uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_tuple_incdec_unc() external view  returns (uint256) { unchecked { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_modifier_incdec_unc() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_incdec_unc(rx); }
    function t_ptr_incdec_unc() external view  returns (uint256) { unchecked { function() internal pure returns (uint256) rp = _w_ptr_incdec_unc; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_mem_incdec_unc() external view  returns (uint256) { unchecked { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 z1 = ry; z1++; uint8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
}
contract Cu64_0 {
    function _s_retdirect_unc_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_unc_all(uint64 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv + uint64(3), pv - uint64(3), pv * uint64(3), uint64(3) + pv, uint64(3) - pv, uint64(3) * pv))); } }
    function _s_retdirect_divmod_all() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_divmod_all(uint64 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv / uint64(3), pv % uint64(3), uint64(3) / pv, uint64(3) % pv))); }
    function _s_retdirect_incdec_unc() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_incdec_unc(uint64 pv) internal view  returns (uint256) { unchecked { uint64 z1 = pv; z1++; uint64 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_local_unc_all() external view  returns (uint256) { unchecked { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(rx + uint64(3), rx - uint64(3), rx * uint64(3), uint64(3) + rx, uint64(3) - rx, uint64(3) * rx))); } }
    function t_retdirect_unc_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_unc_all() + uint64(3), _s_retdirect_unc_all() - uint64(3), _s_retdirect_unc_all() * uint64(3), uint64(3) + _s_retdirect_unc_all(), uint64(3) - _s_retdirect_unc_all(), uint64(3) * _s_retdirect_unc_all()))); } }
    function t_param_unc_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_unc_all(rx); }
    function t_mem_unc_all() external view  returns (uint256) { unchecked { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(ry + uint64(3), ry - uint64(3), ry * uint64(3), uint64(3) + ry, uint64(3) - ry, uint64(3) * ry))); } }
    function t_local_divmod_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return uint256(keccak256(abi.encode(rx / uint64(3), rx % uint64(3), uint64(3) / rx, uint64(3) % rx))); }
    function t_retdirect_divmod_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_divmod_all() / uint64(3), _s_retdirect_divmod_all() % uint64(3), uint64(3) / _s_retdirect_divmod_all(), uint64(3) % _s_retdirect_divmod_all()))); }
    function t_param_divmod_all() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_divmod_all(rx); }
    function t_mem_divmod_all() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; return uint256(keccak256(abi.encode(ry / uint64(3), ry % uint64(3), uint64(3) / ry, uint64(3) % ry))); }
    function t_local_incdec_unc() external view  returns (uint256) { unchecked { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 z1 = rx; z1++; uint64 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_retdirect_incdec_unc() external view  returns (uint256) { unchecked { uint64 z1 = _s_retdirect_incdec_unc(); z1++; uint64 z2 = _s_retdirect_incdec_unc(); --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_param_incdec_unc() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_incdec_unc(rx); }
    function t_mem_incdec_unc() external view  returns (uint256) { unchecked { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 z1 = ry; z1++; uint64 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
}
contract Ci8_0 {
    function _s_retdirect_unc_all() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_unc_all(int8 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv + int8(-2), pv - int8(-2), pv * int8(-2), int8(-2) + pv, int8(-2) - pv, int8(-2) * pv))); } }
    function _s_retdirect_divmod_all() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_divmod_all(int8 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv / int8(-2), pv % int8(-2), int8(-2) / pv, int8(-2) % pv))); }
    function _s_retdirect_incdec_unc() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_incdec_unc(int8 pv) internal view  returns (uint256) { unchecked { int8 z1 = pv; z1++; int8 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_local_unc_all() external view  returns (uint256) { unchecked { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(rx + int8(-2), rx - int8(-2), rx * int8(-2), int8(-2) + rx, int8(-2) - rx, int8(-2) * rx))); } }
    function t_retdirect_unc_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_unc_all() + int8(-2), _s_retdirect_unc_all() - int8(-2), _s_retdirect_unc_all() * int8(-2), int8(-2) + _s_retdirect_unc_all(), int8(-2) - _s_retdirect_unc_all(), int8(-2) * _s_retdirect_unc_all()))); } }
    function t_param_unc_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_unc_all(rx); }
    function t_mem_unc_all() external view  returns (uint256) { unchecked { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(ry + int8(-2), ry - int8(-2), ry * int8(-2), int8(-2) + ry, int8(-2) - ry, int8(-2) * ry))); } }
    function t_local_divmod_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return uint256(keccak256(abi.encode(rx / int8(-2), rx % int8(-2), int8(-2) / rx, int8(-2) % rx))); }
    function t_retdirect_divmod_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_divmod_all() / int8(-2), _s_retdirect_divmod_all() % int8(-2), int8(-2) / _s_retdirect_divmod_all(), int8(-2) % _s_retdirect_divmod_all()))); }
    function t_param_divmod_all() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_divmod_all(rx); }
    function t_mem_divmod_all() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; return uint256(keccak256(abi.encode(ry / int8(-2), ry % int8(-2), int8(-2) / ry, int8(-2) % ry))); }
    function t_local_incdec_unc() external view  returns (uint256) { unchecked { int8 rx; assembly { rx := 0xabcdfd } int8 z1 = rx; z1++; int8 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_retdirect_incdec_unc() external view  returns (uint256) { unchecked { int8 z1 = _s_retdirect_incdec_unc(); z1++; int8 z2 = _s_retdirect_incdec_unc(); --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_param_incdec_unc() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_incdec_unc(rx); }
    function t_mem_incdec_unc() external view  returns (uint256) { unchecked { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 z1 = ry; z1++; int8 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
}
contract Ci24_0 {
    function _s_retdirect_unc_all() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_unc_all(int24 pv) internal view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(pv + int24(-2), pv - int24(-2), pv * int24(-2), int24(-2) + pv, int24(-2) - pv, int24(-2) * pv))); } }
    function _s_retdirect_divmod_all() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_divmod_all(int24 pv) internal view  returns (uint256) { return uint256(keccak256(abi.encode(pv / int24(-2), pv % int24(-2), int24(-2) / pv, int24(-2) % pv))); }
    function _s_retdirect_incdec_unc() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_incdec_unc(int24 pv) internal view  returns (uint256) { unchecked { int24 z1 = pv; z1++; int24 z2 = pv; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_local_unc_all() external view  returns (uint256) { unchecked { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(rx + int24(-2), rx - int24(-2), rx * int24(-2), int24(-2) + rx, int24(-2) - rx, int24(-2) * rx))); } }
    function t_retdirect_unc_all() external view  returns (uint256) { unchecked { return uint256(keccak256(abi.encode(_s_retdirect_unc_all() + int24(-2), _s_retdirect_unc_all() - int24(-2), _s_retdirect_unc_all() * int24(-2), int24(-2) + _s_retdirect_unc_all(), int24(-2) - _s_retdirect_unc_all(), int24(-2) * _s_retdirect_unc_all()))); } }
    function t_param_unc_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_unc_all(rx); }
    function t_mem_unc_all() external view  returns (uint256) { unchecked { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(ry + int24(-2), ry - int24(-2), ry * int24(-2), int24(-2) + ry, int24(-2) - ry, int24(-2) * ry))); } }
    function t_local_divmod_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return uint256(keccak256(abi.encode(rx / int24(-2), rx % int24(-2), int24(-2) / rx, int24(-2) % rx))); }
    function t_retdirect_divmod_all() external view  returns (uint256) { return uint256(keccak256(abi.encode(_s_retdirect_divmod_all() / int24(-2), _s_retdirect_divmod_all() % int24(-2), int24(-2) / _s_retdirect_divmod_all(), int24(-2) % _s_retdirect_divmod_all()))); }
    function t_param_divmod_all() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_divmod_all(rx); }
    function t_mem_divmod_all() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; return uint256(keccak256(abi.encode(ry / int24(-2), ry % int24(-2), int24(-2) / ry, int24(-2) % ry))); }
    function t_local_incdec_unc() external view  returns (uint256) { unchecked { int24 rx; assembly { rx := 0xabcdfffffd } int24 z1 = rx; z1++; int24 z2 = rx; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_retdirect_incdec_unc() external view  returns (uint256) { unchecked { int24 z1 = _s_retdirect_incdec_unc(); z1++; int24 z2 = _s_retdirect_incdec_unc(); --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
    function t_param_incdec_unc() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_incdec_unc(rx); }
    function t_mem_incdec_unc() external view  returns (uint256) { unchecked { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 z1 = ry; z1++; int24 z2 = ry; --z2; return uint256(keccak256(abi.encode(z1, z2))); } }
}
