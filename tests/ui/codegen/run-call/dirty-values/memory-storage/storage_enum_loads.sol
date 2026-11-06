//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740 --allow=2072
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::t_unused => 1
//@ run-call: C::t_local_unused => 1
//@ run-call: C::t_local_asm => 5
//@ run-call: C::t_internal_arg => 5
//@ run-call: C::t_internal_return => 5
//@ run-call: C::t_ternary => 5
//@ run-call: C::t_tuple => 5
//@ run-call: C::t_field_unused => 1
//@ run-call: C::t_field_asm => 5
//@ run-call: C::t_pointer_asm => 5
//@ run-call: C::t_array_asm => 5
//@ run-call: C::t_fixed_array_asm => 5
//@ run-call: C::t_mapping_asm => 5
//@ run-call: C::t_assign_asm => 5
//@ run-call: C::t_swap_asm => 5
//@ run-call: C::t_transient_asm => 5
//@ run-call: C::t_transient_unused => 1
//@ run-call-fail: C::t_compare => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_compare_field => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_compare_transient => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_convert => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_convert_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_encode => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_encode_mapping => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_external_arg => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_external_arg_local => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_external_return => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call: C::t_getter_call => 0
//@ run-call-fail: C::t_store_memory => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_store_storage => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_mapping_key => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_copy_struct => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_copy_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: C::t_copy_fixed_array => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// Every expectation was captured by running the same source through
// solc 0.8.38 via-IR (`--via-ir --optimize`) on anvil. A storage or transient enum is only masked
// at the load, and the range check runs where the value is used, so a load that nothing consumes
// keeps the out-of-range word.

enum E {
    A,
    B
}

struct S {
    uint256 a;
    E e;
}

interface IC {
    function h(E) external pure returns (uint256);
}

contract C {
    E e;
    E transient te;
    S s;
    E[] arr;
    E[2] fixedArr;
    mapping(uint256 => E) mp;

    function dirty() internal {
        s.a = 1;
        arr.push(E.A);
        fixedArr[0] = E.A;
        assembly {
            sstore(e.slot, 5)
            tstore(te.slot, 5)
            sstore(add(s.slot, 1), 5)
            mstore(0, arr.slot)
            sstore(keccak256(0, 32), 5)
            sstore(fixedArr.slot, 5)
            mstore(0, 1)
            mstore(32, mp.slot)
            sstore(keccak256(0, 64), 5)
        }
    }

    function pass() internal view returns (E) {
        return e;
    }

    function take(E x) internal pure returns (uint256 r) {
        assembly { r := x }
    }

    function getter() external view returns (E) {
        return e;
    }

    function h(E) external pure returns (uint256) {
        return 4;
    }

    function t_unused() external returns (uint256) { dirty(); e; return 1; }
    function t_local_unused() external returns (uint256) { dirty(); E x = e; return 1; }
    function t_local_asm() external returns (uint256 r) { dirty(); E x = e; assembly { r := x } }
    function t_internal_arg() external returns (uint256) { dirty(); return take(e); }
    function t_internal_return() external returns (uint256 r) { dirty(); E x = pass(); assembly { r := x } }
    function t_ternary() external returns (uint256 r) { dirty(); E x = gasleft() > 0 ? e : E.A; assembly { r := x } }
    function t_tuple() external returns (uint256 r) { dirty(); (E x, uint256 z) = (e, 1); assembly { r := x } }
    function t_field_unused() external returns (uint256) { dirty(); s.e; return 1; }
    function t_field_asm() external returns (uint256 r) { dirty(); E x = s.e; assembly { r := x } }
    function t_pointer_asm() external returns (uint256 r) { dirty(); S storage p = s; E x = p.e; assembly { r := x } }
    function t_array_asm() external returns (uint256 r) { dirty(); E x = arr[0]; assembly { r := x } }
    function t_fixed_array_asm() external returns (uint256 r) { dirty(); E x = fixedArr[0]; assembly { r := x } }
    function t_mapping_asm() external returns (uint256 r) { dirty(); E x = mp[1]; assembly { r := x } }
    function t_assign_asm() external returns (uint256 r) { dirty(); E x; x = mp[1]; assembly { r := x } }
    function t_swap_asm() external returns (uint256 r) { dirty(); E x = e; E y = E.A; (x, y) = (y, x); assembly { r := y } }
    function t_transient_asm() external returns (uint256 r) { dirty(); E x = te; assembly { r := x } }
    function t_transient_unused() external returns (uint256) { dirty(); E x = te; return 1; }

    function t_compare() external returns (uint256) { dirty(); return e == E.A ? 1 : 2; }
    function t_compare_field() external returns (uint256) { dirty(); return s.e == E.A ? 1 : 2; }
    function t_compare_transient() external returns (uint256) { dirty(); return te == E.A ? 1 : 2; }
    function t_convert() external returns (uint256) { dirty(); return uint256(e); }
    function t_convert_array() external returns (uint256) { dirty(); return uint256(arr[0]); }
    function t_encode() external returns (uint256) { dirty(); return uint256(bytes32(abi.encode(e))); }
    function t_encode_mapping() external returns (uint256) { dirty(); return uint256(bytes32(abi.encode(mp[1]))); }
    function t_external_arg() external returns (uint256) { dirty(); return this.h(e); }
    function t_external_arg_local() external returns (uint256) { dirty(); E x = e; return this.h(x); }
    function t_external_return() external returns (E) { dirty(); return e; }
    function t_getter_call() external returns (uint256) { dirty(); (bool ok, ) = address(this).call(abi.encodeCall(this.getter, ())); return ok ? 1 : 0; }
    function t_store_memory() external returns (uint256) { dirty(); E[] memory m = new E[](1); m[0] = e; return 1; }
    function t_store_storage() external returns (uint256) { dirty(); E[] memory m = new E[](1); arr[0] = s.e; return 1; }
    function t_mapping_key() external returns (uint256) { dirty(); return mk[e]; }
    function t_copy_struct() external returns (uint256) { dirty(); S memory m = s; return 1; }
    function t_copy_array() external returns (uint256) { dirty(); E[] memory m = arr; return 1; }
    function t_copy_fixed_array() external returns (uint256) { dirty(); E[2] memory m = fixedArr; return 1; }

    mapping(E => uint256) mk;
}
