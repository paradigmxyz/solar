pragma solidity ^0.8.24;
// The return data builtins name the latest external call of a function body: its success flag
// (`__call_success`), the length of its return data (`__returndatasize`) and the first 32 bytes of
// that data as a word, zero-padded (`__returndataword`). A helper that makes the call can then
// state on itself the rule it applies to what the callee returned.
library SafeCall {
    // The usual acceptance rule: the call succeeded and returned nothing or the word 1.
    /// @custom:ensures ok == (__call_success && (__returndatasize == 0 || __returndataword == 1))
    function ok_accept(address t, bytes memory d) internal returns (bool ok) {
        assembly {
            let s := call(gas(), t, 0, add(d, 32), mload(d), 0, 32)
            ok := and(s, or(iszero(returndatasize()), and(gt(returndatasize(), 31), eq(mload(0), 1))))
        }
    }

    // A word read from fewer than 32 returned bytes keeps whatever the buffer held before: the
    // helper must check the length, and this one does not.
    /// @custom:ensures ok == (__call_success && (__returndatasize == 0 || __returndataword == 1))
    function bad_accept_short(address t, bytes memory d) internal returns (bool ok) {
        assembly {
            let s := call(gas(), t, 0, add(d, 32), mload(d), 0, 32)
            ok := and(s, or(iszero(returndatasize()), eq(mload(0), 1)))
        }
    }

    // Accepting any return data is not the rule.
    /// @custom:ensures ok == (__call_success && (__returndatasize == 0 || __returndataword == 1))
    function bad_accept_any(address t, bytes memory d) internal returns (bool ok) {
        assembly {
            ok := call(gas(), t, 0, add(d, 32), mload(d), 0, 32)
        }
    }

    /// @custom:ensures ok == (__call_success && __returndatasize == 0)
    function ok_solidity(address t, bytes memory d) internal returns (bool ok) {
        (bool s, bytes memory r) = t.call(d);
        ok = s && r.length == 0;
    }

    /// @custom:ensures ok == __call_success
    function ok_static(address t) internal view returns (bool ok) {
        assembly {
            ok := staticcall(gas(), t, 0, 0, 0, 0)
        }
    }
}

contract Caller {
    // A caller reads the helper's rule from its postcondition.
    /// @custom:ensures ok ==> __call_success
    function ok_through_helper(address t, bytes memory d) external returns (bool ok) {
        ok = SafeCall.ok_accept(t, d);
    }

    // expect: but the body makes none
    /// @custom:ensures __returndatasize == 0
    function bad_no_call() external pure returns (uint256 r) {
        r = 1;
    }

    // expect: needs exactly one external call on every path
    /// @custom:ensures ok == __call_success
    function bad_two_calls(address t) external returns (bool ok) {
        (ok, ) = t.call("");
        (ok, ) = t.call("");
    }

    // expect: needs exactly one external call on every path
    /// @custom:ensures ok == __call_success
    function bad_maybe_call(address t, bool b) external returns (bool ok) {
        if (b) {
            (ok, ) = t.call("");
        }
    }

    // expect: needs exactly one external call on every path
    /// @custom:ensures ok == __call_success
    function bad_loop_call(address t, uint256 n) external returns (bool ok) {
        for (uint256 i = 0; i < n; i++) {
            (ok, ) = t.call("");
        }
    }

    // expect: is only allowed in a function's postconditions
    /// @custom:requires __returndatasize == 0
    function bad_requires(address t) external returns (bool ok) {
        (ok, ) = t.call("");
    }
}
