//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740 --allow=2072
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::t_state_asm => 2
//@ run-call: C::t_transient_asm => 2
//@ run-call: C::t_packed_asm => 2
//@ run-call: C::t_field_asm => 2
//@ run-call: C::t_pointer_asm => 2
//@ run-call: C::t_array_asm => 2
//@ run-call: C::t_fixed_array_asm => 2
//@ run-call: C::t_mapping_asm => 2
//@ run-call: C::t_internal_arg => 2
//@ run-call: C::t_internal_return => 2
//@ run-call: C::t_ternary => 2
//@ run-call: C::t_tuple => 2
//@ run-call: C::t_field_and => 1
//@ run-call: C::t_field_or => 2
//@ run-call: C::t_field_if => 1
//@ run-call: C::t_field_eq_true => 1
//@ run-call: C::t_field_not => 0
//@ run-call: C::t_field_encode => 1
//@ run-call: C::t_array_if => 1
//@ run-call: C::t_mapping_not => 0
//@ run-call: C::t_mapping_encode => 1
//@ run-call: C::t_field_external_return => true

// Every expectation was captured by running the same source through
// solc 0.8.38 via-IR (`--via-ir --optimize`) on anvil. A storage `bool` keeps the masked byte at
// the load, wherever it is read from, and only Solidity uses test it against zero.

struct S {
    uint8 a;
    bool b;
}

contract C {
    bool sb;
    bool transient tb;
    S s;
    bool[] arr;
    bool[2] fixedArr;
    mapping(uint256 => bool) mp;
    struct P {
        uint8 a;
        uint8 c;
        bool b;
    }
    P p;

    function dirty() internal {
        arr.push(false);
        fixedArr[0] = false;
        assembly {
            sstore(sb.slot, 2)
            tstore(tb.slot, 2)
            sstore(s.slot, 0x0200)
            sstore(p.slot, 0x020000)
            mstore(0, arr.slot)
            sstore(keccak256(0, 32), 2)
            sstore(fixedArr.slot, 2)
            mstore(0, 1)
            mstore(32, mp.slot)
            sstore(keccak256(0, 64), 2)
        }
    }

    function pass() internal view returns (bool) {
        return s.b;
    }

    function take(bool x) internal pure returns (uint256 r) {
        assembly { r := x }
    }

    function t_state_asm() external returns (uint256 r) { dirty(); bool x = sb; assembly { r := x } }
    function t_transient_asm() external returns (uint256 r) { dirty(); bool x = tb; assembly { r := x } }
    function t_packed_asm() external returns (uint256 r) { dirty(); bool x = p.b; assembly { r := x } }
    function t_field_asm() external returns (uint256 r) { dirty(); bool x = s.b; assembly { r := x } }
    function t_pointer_asm() external returns (uint256 r) { dirty(); S storage q = s; bool x = q.b; assembly { r := x } }
    function t_array_asm() external returns (uint256 r) { dirty(); bool x = arr[0]; assembly { r := x } }
    function t_fixed_array_asm() external returns (uint256 r) { dirty(); bool x = fixedArr[0]; assembly { r := x } }
    function t_mapping_asm() external returns (uint256 r) { dirty(); bool x = mp[1]; assembly { r := x } }
    function t_internal_arg() external returns (uint256) { dirty(); return take(s.b); }
    function t_internal_return() external returns (uint256 r) { dirty(); bool x = pass(); assembly { r := x } }
    function t_ternary() external returns (uint256 r) { dirty(); bool x = gasleft() > 0 ? s.b : false; assembly { r := x } }
    function t_tuple() external returns (uint256 r) { dirty(); (bool x, uint256 z) = (s.b, 1); assembly { r := x } }

    function t_field_and() external returns (uint256 r) { dirty(); bool x = s.b && true; assembly { r := x } }
    function t_field_or() external returns (uint256 r) { dirty(); bool x = s.b || false; assembly { r := x } }

    function t_field_if() external returns (uint256) { dirty(); if (s.b) return 1; return 0; }
    function t_field_eq_true() external returns (uint256) { dirty(); return s.b == true ? 1 : 0; }
    function t_field_not() external returns (uint256) { dirty(); return !s.b ? 1 : 0; }
    function t_field_encode() external returns (uint256) { dirty(); return uint256(bytes32(abi.encode(s.b))); }
    function t_array_if() external returns (uint256) { dirty(); if (arr[0]) return 1; return 0; }
    function t_mapping_not() external returns (uint256) { dirty(); return !mp[1] ? 1 : 0; }
    function t_mapping_encode() external returns (uint256) { dirty(); return uint256(bytes32(abi.encode(mp[1]))); }
    function t_field_external_return() external returns (bool) { dirty(); return s.b; }
}
