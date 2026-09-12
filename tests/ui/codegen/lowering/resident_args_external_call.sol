//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: first 1, 2, 3, 4, 5, 6 => 49
//@ run-call: second 1, 2, 3, 4, 5, 6 => 49

contract ResidentArgsExternalCall {
    function first(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
        external
        returns (uint256)
    {
        return callAndSum(address(4), gasleft(), a, b, c, d, e, f);
    }

    function second(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f)
        external
        returns (uint256)
    {
        return callAndSum(address(4), gasleft(), a, b, c, d, e, f);
    }

    // Keep each argument and its derived value live while the shared helper reloads CALL's
    // saved target and gas. Both entrypoints must preserve these values through the call.
    // CHECK-LABEL: @module ResidentArgsExternalCall_runtime
    // CHECK: gas
    // CHECK-NEXT: push 4
    // CHECK-NEXT: push [[TARGET:[0-9]+]]
    // CHECK-NEXT: mstore
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: push [[GAS:[0-9]+]]
    // CHECK-NEXT: mstore
    // CHECK-COUNT-6: {{^  add$}}
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push [[TARGET]]
    // CHECK-NEXT: mload
    // CHECK-NEXT: push [[GAS]]
    // CHECK-NEXT: mload
    // CHECK-NEXT: call
    function callAndSum(
        address target,
        uint256 gasAmount,
        uint256 a,
        uint256 b,
        uint256 c,
        uint256 d,
        uint256 e,
        uint256 f
    ) internal returns (uint256) {
        unchecked {
            uint256 aa = a + 1;
            uint256 bb = b + 1;
            uint256 cc = c + 1;
            uint256 dd = d + 1;
            uint256 ee = e + 1;
            uint256 ff = f + 1;
            uint256 ok;
            assembly {
                ok := call(gasAmount, target, 0, 0, 0, 0, 0)
            }
            return a + b + c + d + e + f + aa + bb + cc + dd + ee + ff + ok;
        }
    }
}
