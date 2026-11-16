//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_asm_read => 11259163
//@ run-call: Cu8_0::t_retdirect_asm_read => 11259163
//@ run-call: Cu8_0::t_ret_asm_read => 11259163
//@ run-call: Cu8_0::t_param_asm_read => 11259163
//@ run-call: Cu8_0::t_copy_asm_read => 11259163
//@ run-call: Cu8_0::t_local_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_retdirect_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ret_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_param_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_copy_asm_read_typed_also => 11259164
//@ run-call: Cu64_0::t_local_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_retdirect_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_param_asm_read => 811306251105819789623323
//@ run-call: Cu64_0::t_local_asm_read_typed_also => 811306251105819789623324
//@ run-call: Cu64_0::t_retdirect_asm_read_typed_also => 811306251105819789623324
//@ run-call: Cu64_0::t_param_asm_read_typed_also => 811306251105819789623324
//@ run-call: Ci8_0::t_local_asm_read => 11259389
//@ run-call: Ci8_0::t_retdirect_asm_read => 11259389
//@ run-call: Ci8_0::t_param_asm_read => 11259389
//@ run-call: Ci8_0::t_local_asm_read_typed_also => 11259394
//@ run-call: Ci8_0::t_retdirect_asm_read_typed_also => 11259394
//@ run-call: Ci8_0::t_param_asm_read_typed_also => 11259394
//@ run-call: Ci24_0::t_local_asm_read => 737895514109
//@ run-call: Ci24_0::t_retdirect_asm_read => 737895514109
//@ run-call: Ci24_0::t_param_asm_read => 737895514109
//@ run-call: Ci24_0::t_local_asm_read_typed_also => 737895514114
//@ run-call: Ci24_0::t_retdirect_asm_read_typed_also => 737895514114
//@ run-call: Ci24_0::t_param_asm_read_typed_also => 737895514114
//@ run-call: Caddr_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ret_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_copy_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ret_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_copy_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddrp_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddrp_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_local_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_retdirect_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_param_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Cctr_0::t_local_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_retdirect_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cctr_0::t_param_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Cbool_0::t_local_asm_read => 256
//@ run-call: Cbool_0::t_retdirect_asm_read => 256
//@ run-call: Cbool_0::t_param_asm_read => 256
//@ run-call: Cbool_0::t_local_asm_read_typed_also => 259
//@ run-call: Cbool_0::t_retdirect_asm_read_typed_also => 259
//@ run-call: Cbool_0::t_param_asm_read_typed_also => 259
//@ run-call: Cb1_0::t_local_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_retdirect_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_param_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723379711
//@ run-call: Cb1_0::t_local_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb1_0::t_retdirect_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb1_0::t_param_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723379714
//@ run-call: Cb4_0::t_local_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_retdirect_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ret_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_param_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_copy_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_local_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_retdirect_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ret_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_param_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_copy_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb20_0::t_local_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_retdirect_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_param_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088422861
//@ run-call: Cb20_0::t_local_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cb20_0::t_retdirect_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cb20_0::t_param_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088422866
//@ run-call: Cen_0::t_local_asm_read => 257
//@ run-call: Cen_0::t_retdirect_asm_read => 257
//@ run-call: Cen_0::t_ret_asm_read => 257
//@ run-call: Cen_0::t_param_asm_read => 257
//@ run-call: Cen_0::t_copy_asm_read => 257
//@ run-call-fail: Cen_0::t_local_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ret_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_copy_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_asm_read => 11259163
//@ run-call: Cud_0::t_retdirect_asm_read => 11259163
//@ run-call: Cud_0::t_param_asm_read => 11259163
//@ run-call: Cud_0::t_local_asm_read_typed_also => 11259164
//@ run-call: Cud_0::t_retdirect_asm_read_typed_also => 11259164
//@ run-call: Cud_0::t_param_asm_read_typed_also => 11259164

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: dirt surviving copies, calls, returns and joins into an assembly read.

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
    function _s_retdirect_asm_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_asm_read() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read(uint8 pv) internal view  returns (uint256) { uint8 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _s_ret_asm_read_typed_also() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read_typed_also(uint8 pv) internal view  returns (uint256) { uint8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { uint8 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view  returns (uint256) { uint8 rx = _s_ret_asm_read(); uint8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { uint8 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view  returns (uint256) { uint8 rx = _s_ret_asm_read_typed_also(); uint8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } uint8 ry = rx; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cu64_0 {
    function _s_retdirect_asm_read() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_asm_read(uint64 pv) internal view  returns (uint256) { uint64 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_asm_read_typed_also(uint64 pv) internal view  returns (uint256) { uint64 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { uint64 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } uint64 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { uint64 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_asm_read_typed_also(rx); }
}
contract Ci8_0 {
    function _s_retdirect_asm_read() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_asm_read(int8 pv) internal view  returns (uint256) { int8 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_asm_read_typed_also(int8 pv) internal view  returns (uint256) { int8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { int8 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } int8 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { int8 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_asm_read_typed_also(rx); }
}
contract Ci24_0 {
    function _s_retdirect_asm_read() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_asm_read(int24 pv) internal view  returns (uint256) { int24 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_asm_read_typed_also(int24 pv) internal view  returns (uint256) { int24 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { int24 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } int24 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { int24 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_asm_read_typed_also(rx); }
}
contract Caddr_0 {
    function _s_retdirect_asm_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_asm_read() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(address pv) internal view  returns (uint256) { address t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _s_ret_asm_read_typed_also() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(address pv) internal view  returns (uint256) { address t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { address t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view  returns (uint256) { address rx = _s_ret_asm_read(); address t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { address t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view  returns (uint256) { address rx = _s_ret_asm_read_typed_also(); address t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address ry = rx; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddrp_0 {
    function _s_retdirect_asm_read() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(address payable pv) internal view  returns (uint256) { address payable t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(address payable pv) internal view  returns (uint256) { address payable t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { address payable t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } address payable t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { address payable t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
}
contract Cctr_0 {
    function _s_retdirect_asm_read() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read(IC pv) internal view  returns (uint256) { IC t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_asm_read_typed_also(IC pv) internal view  returns (uint256) { IC t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { IC t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } IC t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { IC t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_asm_read_typed_also(rx); }
}
contract Cbool_0 {
    function _s_retdirect_asm_read() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_asm_read(bool pv) internal view  returns (uint256) { bool t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_asm_read_typed_also(bool pv) internal view  returns (uint256) { bool t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { bool t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } bool t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { bool t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_asm_read_typed_also(rx); }
}
contract Cb1_0 {
    function _s_retdirect_asm_read() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_asm_read(bytes1 pv) internal view  returns (uint256) { bytes1 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_asm_read_typed_also(bytes1 pv) internal view  returns (uint256) { bytes1 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { bytes1 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } bytes1 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { bytes1 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_asm_read_typed_also(rx); }
}
contract Cb4_0 {
    function _s_retdirect_asm_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_asm_read() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_asm_read(bytes4 pv) internal view  returns (uint256) { bytes4 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _s_ret_asm_read_typed_also() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_asm_read_typed_also(bytes4 pv) internal view  returns (uint256) { bytes4 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { bytes4 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view  returns (uint256) { bytes4 rx = _s_ret_asm_read(); bytes4 t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { bytes4 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view  returns (uint256) { bytes4 rx = _s_ret_asm_read_typed_also(); bytes4 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } bytes4 ry = rx; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb20_0 {
    function _s_retdirect_asm_read() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_asm_read(bytes20 pv) internal view  returns (uint256) { bytes20 t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_asm_read_typed_also(bytes20 pv) internal view  returns (uint256) { bytes20 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { bytes20 t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } bytes20 t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { bytes20 t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_asm_read_typed_also(rx); }
}
contract Cen_0 {
    function _s_retdirect_asm_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_asm_read() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_asm_read(E pv) internal view  returns (uint256) { E t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (E r) { assembly { r := 0x101 } }
    function _s_ret_asm_read_typed_also() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_asm_read_typed_also(E pv) internal view  returns (uint256) { E t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { E t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_ret_asm_read() external view  returns (uint256) { E rx = _s_ret_asm_read(); E t = rx; uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_asm_read(rx); }
    function t_copy_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_local_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { E t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_ret_asm_read_typed_also() external view  returns (uint256) { E rx = _s_ret_asm_read_typed_also(); E t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_asm_read_typed_also(rx); }
    function t_copy_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } E ry = rx; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cud_0 {
    function _s_retdirect_asm_read() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read(U pv) internal view  returns (uint256) { U t = pv; uint256 r; assembly { r := t } return r; }
    function _s_retdirect_asm_read_typed_also() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_asm_read_typed_also(U pv) internal view  returns (uint256) { U t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_local_asm_read() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U t = rx; uint256 r; assembly { r := t } return r; }
    function t_retdirect_asm_read() external view  returns (uint256) { U t = _s_retdirect_asm_read(); uint256 r; assembly { r := t } return r; }
    function t_param_asm_read() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_asm_read(rx); }
    function t_local_asm_read_typed_also() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } U t = rx; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_retdirect_asm_read_typed_also() external view  returns (uint256) { U t = _s_retdirect_asm_read_typed_also(); uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_param_asm_read_typed_also() external view  returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_asm_read_typed_also(rx); }
}
