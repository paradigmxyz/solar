//@ revisions: ir run
//@[ir] compile-flags: -Ogas -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@[run] compile-flags: -Ogas
//@[run] run-call: first 1, 2, 3, 4, 5, 6 => 49
//@[run] run-call: second 1, 2, 3, 4, 5, 6 => 49

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

    // Keep each source argument and its derived value live across CALL. This
    // leaves the stack-only target and gas operands close to the DUP16 limit
    // before the six CALL operands are materialized.
    // CHECK: dup14
    // CHECK-NEXT: dup14
    // CHECK-COUNT-5: push 0
    // CHECK: call
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
