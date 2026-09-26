//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: payload 0x000000000000000000000000000000000000dEaD, 1 => 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead0000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: short 0x000000000000000000000000000000000000dEaD, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// Building a call payload in memory the caller owns. Each write is bounds
// checked against the buffer, so a buffer one word too short fails with
// `Panic(0x32)` instead of writing past it. The checks fold into the
// constant length, so the payload is three stores.
// CHECK-LABEL: fn @payload
// CHECK: mstore
// CHECK: mstore
// CHECK: mstore
// CHECK-NOT: mstore8
import {Abi} from "solar:core/v1/Abi.sol";

contract Safe {
    function payload(address to, uint256 amount) public pure returns (bytes memory out) {
        out = new bytes(Abi.SELECTOR + 2 * Abi.WORD);
        uint256 at = Abi.encodeSelectorInto(out, 0, bytes4(0xa9059cbb));
        at += Abi.encodeInto(out, at, to);
        Abi.encodeInto(out, at, amount);
    }

    function short(address to, uint256 amount) public pure returns (bytes memory out) {
        out = new bytes(Abi.SELECTOR + Abi.WORD);
        uint256 at = Abi.encodeSelectorInto(out, 0, bytes4(0xa9059cbb));
        at += Abi.encodeInto(out, at, to);
        Abi.encodeInto(out, at, amount);
    }
}
