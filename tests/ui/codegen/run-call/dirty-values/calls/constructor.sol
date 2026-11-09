//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_ctor_arg => 27
//@ run-call: Cu8_0::t_retdirect_ctor_arg => 27
//@ run-call: Cu8_0::t_param_ctor_arg => 27
//@ run-call: Cu8_0::t_mem_ctor_arg => 27
//@ run-call: Cu64_0::t_local_ctor_arg => 27
//@ run-call: Cu64_0::t_retdirect_ctor_arg => 27
//@ run-call: Cu64_0::t_param_ctor_arg => 27
//@ run-call: Cu64_0::t_mem_ctor_arg => 27
//@ run-call: Ci8_0::t_local_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_retdirect_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_param_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci8_0::t_mem_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_local_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_retdirect_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_param_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Ci24_0::t_mem_ctor_arg => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: Caddr_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddr_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Caddrp_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_local_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_retdirect_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_param_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cctr_0::t_mem_ctor_arg => 103929005307130220006098923584552504982110632080
//@ run-call: Cbool_0::t_local_ctor_arg => 1
//@ run-call: Cbool_0::t_retdirect_ctor_arg => 1
//@ run-call: Cbool_0::t_param_ctor_arg => 1
//@ run-call: Cbool_0::t_mem_ctor_arg => 1
//@ run-call: Cb1_0::t_local_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_retdirect_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_param_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb1_0::t_mem_ctor_arg => 77345497107738552411838431392522000948863935069627095510731433067785723314176
//@ run-call: Cb4_0::t_local_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_retdirect_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_param_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb4_0::t_mem_ctor_arg => 100720434702924942364018397558880508427273416251376888068364465368051161759744
//@ run-call: Cb20_0::t_local_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_retdirect_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_param_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call: Cb20_0::t_mem_ctor_arg => 8234104122419153896766082834368325185836758793843649141158396535749088378880
//@ run-call-fail: Cen_0::t_local_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_retdirect_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_param_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Cen_0::t_mem_ctor_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: Cud_0::t_local_ctor_arg => 27
//@ run-call: Cud_0::t_retdirect_ctor_arg => 27
//@ run-call: Cud_0::t_param_ctor_arg => 27
//@ run-call: Cud_0::t_mem_ctor_arg => 27

// Generated matrix. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR: external calls, try/catch, call options, constructors and address operands.

interface IC {
    function f() external;
}

enum E {
    A,
    B,
    C
}

type U is uint8;

contract Ctor_u8 {
    uint256 public got;

    constructor(uint8 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_u64 {
    uint256 public got;

    constructor(uint64 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_i8 {
    uint256 public got;

    constructor(int8 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_i24 {
    uint256 public got;

    constructor(int24 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_addr {
    uint256 public got;

    constructor(address v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_addrp {
    uint256 public got;

    constructor(address payable v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_ctr {
    uint256 public got;

    constructor(IC v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_bool {
    uint256 public got;

    constructor(bool v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_b1 {
    uint256 public got;

    constructor(bytes1 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_b4 {
    uint256 public got;

    constructor(bytes4 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_b20 {
    uint256 public got;

    constructor(bytes20 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_en {
    uint256 public got;

    constructor(E v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Ctor_ud {
    uint256 public got;

    constructor(U v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Cu8_0 {
    function _s_retdirect_ctor_arg() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_ctor_arg(uint8 pv) internal   returns (uint256) { Ctor_u8 k = new Ctor_u8(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Ctor_u8 k = new Ctor_u8(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_u8 k = new Ctor_u8(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Ctor_u8 k = new Ctor_u8(ry); uint256 r = k.got(); return r; }
}
contract Cu64_0 {
    function _s_retdirect_ctor_arg() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_ctor_arg(uint64 pv) internal   returns (uint256) { Ctor_u64 k = new Ctor_u64(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Ctor_u64 k = new Ctor_u64(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_u64 k = new Ctor_u64(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Ctor_u64 k = new Ctor_u64(ry); uint256 r = k.got(); return r; }
}
contract Ci8_0 {
    function _s_retdirect_ctor_arg() internal view returns (int8 r) { assembly { r := 0xabcdfd } }
    function _p_param_ctor_arg(int8 pv) internal   returns (uint256) { Ctor_i8 k = new Ctor_i8(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } Ctor_i8 k = new Ctor_i8(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_i8 k = new Ctor_i8(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { int8 rx; assembly { rx := 0xabcdfd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { int8[] memory ra = new int8[](1); assembly { mstore(add(ra, 32), 0xabcdfd) } int8 ry = ra[0]; Ctor_i8 k = new Ctor_i8(ry); uint256 r = k.got(); return r; }
}
contract Ci24_0 {
    function _s_retdirect_ctor_arg() internal view returns (int24 r) { assembly { r := 0xabcdfffffd } }
    function _p_param_ctor_arg(int24 pv) internal   returns (uint256) { Ctor_i24 k = new Ctor_i24(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } Ctor_i24 k = new Ctor_i24(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_i24 k = new Ctor_i24(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { int24 rx; assembly { rx := 0xabcdfffffd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { int24[] memory ra = new int24[](1); assembly { mstore(add(ra, 32), 0xabcdfffffd) } int24 ry = ra[0]; Ctor_i24 k = new Ctor_i24(ry); uint256 r = k.got(); return r; }
}
contract Caddr_0 {
    function _s_retdirect_ctor_arg() internal view returns (address r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(address pv) internal   returns (uint256) { Ctor_addr k = new Ctor_addr(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_addr k = new Ctor_addr(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_addr k = new Ctor_addr(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { address rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { address[] memory ra = new address[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address ry = ra[0]; Ctor_addr k = new Ctor_addr(ry); uint256 r = k.got(); return r; }
}
contract Caddrp_0 {
    function _s_retdirect_ctor_arg() internal view returns (address payable r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(address payable pv) internal   returns (uint256) { Ctor_addrp k = new Ctor_addrp(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_addrp k = new Ctor_addrp(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_addrp k = new Ctor_addrp(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { address payable rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { address payable[] memory ra = new address payable[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } address payable ry = ra[0]; Ctor_addrp k = new Ctor_addrp(ry); uint256 r = k.got(); return r; }
}
contract Cctr_0 {
    function _s_retdirect_ctor_arg() internal view returns (IC r) { assembly { r := 0xabcd1234567890123456789012345678901234567890 } }
    function _p_param_ctor_arg(IC pv) internal   returns (uint256) { Ctor_ctr k = new Ctor_ctr(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } Ctor_ctr k = new Ctor_ctr(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_ctr k = new Ctor_ctr(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { IC rx; assembly { rx := 0xabcd1234567890123456789012345678901234567890 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { IC[] memory ra = new IC[](1); assembly { mstore(add(ra, 32), 0xabcd1234567890123456789012345678901234567890) } IC ry = ra[0]; Ctor_ctr k = new Ctor_ctr(ry); uint256 r = k.got(); return r; }
}
contract Cbool_0 {
    function _s_retdirect_ctor_arg() internal view returns (bool r) { assembly { r := 0x100 } }
    function _p_param_ctor_arg(bool pv) internal   returns (uint256) { Ctor_bool k = new Ctor_bool(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } Ctor_bool k = new Ctor_bool(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_bool k = new Ctor_bool(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { bool rx; assembly { rx := 0x100 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { bool[] memory ra = new bool[](1); assembly { mstore(add(ra, 32), 0x100) } bool ry = ra[0]; Ctor_bool k = new Ctor_bool(ry); uint256 r = k.got(); return r; }
}
contract Cb1_0 {
    function _s_retdirect_ctor_arg() internal view returns (bytes1 r) { assembly { r := 0xab0000000000000000000000000000000000000000000000000000000000ffff } }
    function _p_param_ctor_arg(bytes1 pv) internal   returns (uint256) { Ctor_b1 k = new Ctor_b1(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } Ctor_b1 k = new Ctor_b1(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_b1 k = new Ctor_b1(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { bytes1 rx; assembly { rx := 0xab0000000000000000000000000000000000000000000000000000000000ffff } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { bytes1[] memory ra = new bytes1[](1); assembly { mstore(add(ra, 32), 0xab0000000000000000000000000000000000000000000000000000000000ffff) } bytes1 ry = ra[0]; Ctor_b1 k = new Ctor_b1(ry); uint256 r = k.got(); return r; }
}
contract Cb4_0 {
    function _s_retdirect_ctor_arg() internal view returns (bytes4 r) { assembly { r := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } }
    function _p_param_ctor_arg(bytes4 pv) internal   returns (uint256) { Ctor_b4 k = new Ctor_b4(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } Ctor_b4 k = new Ctor_b4(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_b4 k = new Ctor_b4(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { bytes4 rx; assembly { rx := 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { bytes4[] memory ra = new bytes4[](1); assembly { mstore(add(ra, 32), 0xdeadbeef00000000000000000000000000000000000000000000000000abcdef) } bytes4 ry = ra[0]; Ctor_b4 k = new Ctor_b4(ry); uint256 r = k.got(); return r; }
}
contract Cb20_0 {
    function _s_retdirect_ctor_arg() internal view returns (bytes20 r) { assembly { r := 0x123456789012345678901234567890123456789000000000000000000000abcd } }
    function _p_param_ctor_arg(bytes20 pv) internal   returns (uint256) { Ctor_b20 k = new Ctor_b20(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } Ctor_b20 k = new Ctor_b20(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_b20 k = new Ctor_b20(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { bytes20 rx; assembly { rx := 0x123456789012345678901234567890123456789000000000000000000000abcd } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { bytes20[] memory ra = new bytes20[](1); assembly { mstore(add(ra, 32), 0x123456789012345678901234567890123456789000000000000000000000abcd) } bytes20 ry = ra[0]; Ctor_b20 k = new Ctor_b20(ry); uint256 r = k.got(); return r; }
}
contract Cen_0 {
    function _s_retdirect_ctor_arg() internal view returns (E r) { assembly { r := 0x101 } }
    function _p_param_ctor_arg(E pv) internal   returns (uint256) { Ctor_en k = new Ctor_en(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } Ctor_en k = new Ctor_en(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_en k = new Ctor_en(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { E rx; assembly { rx := 0x101 } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { E[] memory ra = new E[](1); assembly { mstore(add(ra, 32), 0x101) } E ry = ra[0]; Ctor_en k = new Ctor_en(ry); uint256 r = k.got(); return r; }
}
contract Cud_0 {
    function _s_retdirect_ctor_arg() internal view returns (U r) { assembly { r := 0xabcd1b } }
    function _p_param_ctor_arg(U pv) internal   returns (uint256) { Ctor_ud k = new Ctor_ud(pv); uint256 r = k.got(); return r; }
    function t_local_ctor_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } Ctor_ud k = new Ctor_ud(rx); uint256 r = k.got(); return r; }
    function t_retdirect_ctor_arg() external   returns (uint256) { Ctor_ud k = new Ctor_ud(_s_retdirect_ctor_arg()); uint256 r = k.got(); return r; }
    function t_param_ctor_arg() external   returns (uint256) { U rx; assembly { rx := 0xabcd1b } return _p_param_ctor_arg(rx); }
    function t_mem_ctor_arg() external   returns (uint256) { U[] memory ra = new U[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } U ry = ra[0]; Ctor_ud k = new Ctor_ud(ry); uint256 r = k.got(); return r; }
}
