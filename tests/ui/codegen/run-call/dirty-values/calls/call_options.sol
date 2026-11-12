//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Cu8_0::t_local_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_retdirect_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_param_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_mem_opt_value; value=100 => 27
//@ run-call: Cu8_0::t_local_opt_gas => 1
//@ run-call: Cu8_0::t_retdirect_opt_gas => 1
//@ run-call: Cu8_0::t_param_opt_gas => 1
//@ run-call: Cu8_0::t_mem_opt_gas => 1
//@ run-call: Cu8_0::t_local_opt_salt => 1
//@ run-call: Cu8_0::t_retdirect_opt_salt => 1
//@ run-call: Cu8_0::t_param_opt_salt => 1
//@ run-call: Cu8_0::t_mem_opt_salt => 1
//@ run-call: Cu64_0::t_local_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_retdirect_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_param_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_mem_opt_value; value=100 => 27
//@ run-call: Cu64_0::t_local_opt_gas => 1
//@ run-call: Cu64_0::t_retdirect_opt_gas => 1
//@ run-call: Cu64_0::t_param_opt_gas => 1
//@ run-call: Cu64_0::t_mem_opt_gas => 1
//@ run-call: Cu64_0::t_local_opt_salt => 1
//@ run-call: Cu64_0::t_retdirect_opt_salt => 1
//@ run-call: Cu64_0::t_param_opt_salt => 1
//@ run-call: Cu64_0::t_mem_opt_salt => 1

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

contract Callee_u8 {
    function f(uint8 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint8 a, uint8 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint8 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_u8 {
    uint256 public got;

    constructor(uint8 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Callee_u64 {
    function f(uint64 v) external pure returns (uint256 r) {
        assembly { r := v }
    }

    function f2(uint64 a, uint64 b) external pure returns (uint256 r) {
        a;
        assembly { r := b }
    }

    function fail(uint64 v) external pure {
        v;
        revert();
    }

    function g() external payable returns (uint256) {
        return msg.value;
    }

    function h() external view returns (uint256) {
        return gasleft() > 0 ? 1 : 0;
    }
}

contract Ctor_u64 {
    uint256 public got;

    constructor(uint64 v) {
        assembly { sstore(got.slot, v) }
    }
}

contract Cu8_0 {
    function _s_retdirect_opt_value() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_value(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: pv}(); return r; }
    function _s_retdirect_opt_gas() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_gas(uint8 pv) internal   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + pv}(); return r; }
    function _s_retdirect_opt_salt() internal view returns (uint8 r) { assembly { r := 0xabcd1b } }
    function _p_param_opt_salt(uint8 pv) internal   returns (uint256) { Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(pv))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(pv)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_local_opt_value() external payable  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: rx}(); return r; }
    function t_retdirect_opt_value() external payable  returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: _s_retdirect_opt_value()}(); return r; }
    function t_param_opt_value() external payable  returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_value(rx); }
    function t_mem_opt_value() external payable  returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.g{value: ry}(); return r; }
    function t_local_opt_gas() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + rx}(); return r; }
    function t_retdirect_opt_gas() external   returns (uint256) { Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + _s_retdirect_opt_gas()}(); return r; }
    function t_param_opt_gas() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_gas(rx); }
    function t_mem_opt_gas() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Callee_u8 c = new Callee_u8(); uint256 r = c.h{gas: 100000 + ry}(); return r; }
    function t_local_opt_salt() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(rx))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(rx)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_retdirect_opt_salt() external   returns (uint256) { Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(_s_retdirect_opt_salt()))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(_s_retdirect_opt_salt())), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
    function t_param_opt_salt() external   returns (uint256) { uint8 rx; assembly { rx := 0xabcd1b } return _p_param_opt_salt(rx); }
    function t_mem_opt_salt() external   returns (uint256) { uint8[] memory ra = new uint8[](1); assembly { mstore(add(ra, 32), 0xabcd1b) } uint8 ry = ra[0]; Ctor_u8 k = new Ctor_u8{salt: bytes32(uint256(ry))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(ry)), keccak256(abi.encodePacked(type(Ctor_u8).creationCode, abi.encode(uint8(1))))))))); return address(k) == p ? 1 : 0; }
}
contract Cu64_0 {
    function _s_retdirect_opt_value() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_value(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: pv}(); return r; }
    function _s_retdirect_opt_gas() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_gas(uint64 pv) internal   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + pv}(); return r; }
    function _s_retdirect_opt_salt() internal view returns (uint64 r) { assembly { r := 0xabcd000000000000001b } }
    function _p_param_opt_salt(uint64 pv) internal   returns (uint256) { Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(pv))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(pv)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_local_opt_value() external payable  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: rx}(); return r; }
    function t_retdirect_opt_value() external payable  returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: _s_retdirect_opt_value()}(); return r; }
    function t_param_opt_value() external payable  returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_value(rx); }
    function t_mem_opt_value() external payable  returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.g{value: ry}(); return r; }
    function t_local_opt_gas() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + rx}(); return r; }
    function t_retdirect_opt_gas() external   returns (uint256) { Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + _s_retdirect_opt_gas()}(); return r; }
    function t_param_opt_gas() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_gas(rx); }
    function t_mem_opt_gas() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Callee_u64 c = new Callee_u64(); uint256 r = c.h{gas: 100000 + ry}(); return r; }
    function t_local_opt_salt() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(rx))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(rx)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_retdirect_opt_salt() external   returns (uint256) { Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(_s_retdirect_opt_salt()))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(_s_retdirect_opt_salt())), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
    function t_param_opt_salt() external   returns (uint256) { uint64 rx; assembly { rx := 0xabcd000000000000001b } return _p_param_opt_salt(rx); }
    function t_mem_opt_salt() external   returns (uint256) { uint64[] memory ra = new uint64[](1); assembly { mstore(add(ra, 32), 0xabcd000000000000001b) } uint64 ry = ra[0]; Ctor_u64 k = new Ctor_u64{salt: bytes32(uint256(ry))}(1); address p = address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), bytes32(uint256(ry)), keccak256(abi.encodePacked(type(Ctor_u64).creationCode, abi.encode(uint64(1))))))))); return address(k) == p ? 1 : 0; }
}
