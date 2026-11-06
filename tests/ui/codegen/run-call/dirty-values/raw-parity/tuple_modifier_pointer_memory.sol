//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_tuple_asm_read => 11259163
//@ run-call: Cu8_0::t_modifier_asm_read => 11259163
//@ run-call: Cu8_0::t_ptr_asm_read => 11259163
//@ run-call: Cu8_0::t_mem_asm_read => 27
//@ run-call: Cu8_0::t_tuple_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_modifier_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_ptr_asm_read_typed_also => 11259164
//@ run-call: Cu8_0::t_mem_asm_read_typed_also => 28
//@ run-call: Cu64_0::t_mem_asm_read => 27
//@ run-call: Cu64_0::t_mem_asm_read_typed_also => 28
//@ run-call: Ci8_0::t_mem_asm_read => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call-fail: Ci8_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Ci24_0::t_mem_asm_read => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call-fail: Ci24_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: Caddr_0::t_tuple_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_modifier_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_ptr_asm_read => 64278407440455748375736268726618428039992551283259536
//@ run-call: Caddr_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_tuple_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_modifier_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_ptr_asm_read_typed_also => 64278407440455748375736268726618428039992551283259538
//@ run-call: Caddr_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Caddrp_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Cctr_0::t_mem_asm_read => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_asm_read_typed_also => 103929005307130220006098923584552504982110632082
//@ run-call: Cbool_0::t_mem_asm_read => 1
//@ run-call: Cbool_0::t_mem_asm_read_typed_also => 4
//@ run-call: Cb1_0::t_mem_asm_read => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_asm_read_typed_also => 77345497107738552411838431392522000948863935069627095510731433067785723314179
//@ run-call: Cb4_0::t_tuple_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_modifier_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_ptr_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051173019119
//@ run-call: Cb4_0::t_mem_asm_read => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_tuple_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_modifier_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_ptr_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051173019124
//@ run-call: Cb4_0::t_mem_asm_read_typed_also => 100720434702924942364018397558880508427273416251376888068364465368051161759749
//@ run-call: Cb20_0::t_mem_asm_read => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_asm_read_typed_also => 8234104122419153896766082834368325185836758793843649141158396535749088378885
//@ run-call: Cen_0::t_tuple_asm_read => 257
//@ run-call: Cen_0::t_modifier_asm_read => 257
//@ run-call: Cen_0::t_ptr_asm_read => 257
//@ run-call-fail: Cen_0::t_mem_asm_read => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_tuple_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_modifier_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_ptr_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_asm_read_typed_also => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_mem_asm_read => 27
//@ run-call: Cud_0::t_mem_asm_read_typed_also => 28

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
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(uint8 pv) { uint8 t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(uint8 rd) internal _m_modifier_asm_read(rd)  returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xabcd1b; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(uint8 pv) { uint8 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(uint8 rd) internal _m_modifier_asm_read_typed_also(rd)  returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xabcd1b; }
    function t_tuple_asm_read() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read_typed_also() external view  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } (uint8 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (uint8) rq; assembly { rq := rp } uint8 ry = rq(); uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; uint8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cu64_0 {
    function t_mem_asm_read() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; uint64 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Ci8_0 {
    function t_mem_asm_read() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; int8 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Ci24_0 {
    function t_mem_asm_read() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; int24 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddr_0 {
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(address pv) { address t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(address rd) internal _m_modifier_asm_read(rd)  returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(address pv) { address t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(address rd) internal _m_modifier_asm_read_typed_also(rd)  returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xabcd1234567890123456789012345678901234567890; }
    function t_tuple_asm_read() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); address t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read_typed_also() external view  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } (address ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external  returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (address) rq; assembly { rq := rp } address ry = rq(); address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; address t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Caddrp_0 {
    function t_mem_asm_read() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; address payable t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cctr_0 {
    function t_mem_asm_read() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; IC t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cbool_0 {
    function t_mem_asm_read() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; bool t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb1_0 {
    function t_mem_asm_read() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; bytes1 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb4_0 {
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(bytes4 pv) { bytes4 t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(bytes4 rd) internal _m_modifier_asm_read(rd)  returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(bytes4 pv) { bytes4 t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(bytes4 rd) internal _m_modifier_asm_read_typed_also(rd)  returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef; }
    function t_tuple_asm_read() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read_typed_also() external view  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } (bytes4 ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external  returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (bytes4) rq; assembly { rq := rp } bytes4 ry = rq(); bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; bytes4 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cb20_0 {
    function t_mem_asm_read() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; bytes20 t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cen_0 {
    uint256 _res_modifier_asm_read;
    modifier _m_modifier_asm_read(E pv) { E t = pv; uint256 r; assembly { r := t } _res_modifier_asm_read = r; _; }
    function _e_modifier_asm_read(E rd) internal _m_modifier_asm_read(rd)  returns (uint256) { return _res_modifier_asm_read; }
    function _w_ptr_asm_read() internal pure returns (uint256) { return 0x101; }
    uint256 _res_modifier_asm_read_typed_also;
    modifier _m_modifier_asm_read_typed_also(E pv) { E t = pv; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; _res_modifier_asm_read_typed_also = r2; _; }
    function _e_modifier_asm_read_typed_also(E rd) internal _m_modifier_asm_read_typed_also(rd)  returns (uint256) { return _res_modifier_asm_read_typed_also; }
    function _w_ptr_asm_read_typed_also() internal pure returns (uint256) { return 0x101; }
    function t_tuple_asm_read() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_modifier_asm_read() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_asm_read(rx); }
    function t_ptr_asm_read() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); E t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E t = ry; uint256 r; assembly { r := t } return r; }
    function t_tuple_asm_read_typed_also() external view  returns (uint256) { E rx; assembly { rx := 0x101 } (E ry, uint256 rz) = (rx, 1); if (rz != 1) return 0; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_modifier_asm_read_typed_also() external  returns (uint256) { E rx; assembly { rx := 0x101 } return _e_modifier_asm_read_typed_also(rx); }
    function t_ptr_asm_read_typed_also() external view  returns (uint256) { function() internal pure returns (uint256) rp = _w_ptr_asm_read_typed_also; function() internal pure returns (E) rq; assembly { rq := rp } E ry = rq(); E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; E t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
contract Cud_0 {
    function t_mem_asm_read() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U t = ry; uint256 r; assembly { r := t } return r; }
    function t_mem_asm_read_typed_also() external view  returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; U t = ry; uint256 r; assembly { r := t } uint256 r2 = r + uint256(keccak256(abi.encode(t))) % 7; return r2; }
}
