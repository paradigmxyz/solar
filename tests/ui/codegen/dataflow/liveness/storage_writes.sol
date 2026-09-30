//@ compile-flags: -Zdataflow=liveness
//@ filecheck:
// Backward liveness of storage writes. A write is dead when every later path overwrites it
// or reverts first. Internal calls read only their summarized footprint, so a callee that
// reads another slot keeps the write dead; external calls may reenter and read anything.

// CHECK-LABEL: fn @overwritten:
// CHECK-NEXT: sstore 0, 1  ; dead
// CHECK-NEXT: sstore 1, arg0  ; live
// CHECK-NEXT: sstore 0, 2  ; live
// CHECK-LABEL: fn @acrossCallee:
// CHECK-NEXT: sstore 0, 1  ; dead
// CHECK-NEXT: sstore 0, 2  ; live
// CHECK-LABEL: fn @readByCallee:
// CHECK-NEXT: sstore 0, 1  ; live
// CHECK-NEXT: sstore 0, 2  ; live
// CHECK-LABEL: fn @beforeRevert:
// CHECK-NEXT: sstore 0, arg0  ; dead
// CHECK-LABEL: fn @acrossExternalCall:
// CHECK-NEXT: sstore {{.*}}  ; live
// CHECK-NEXT: sstore {{.*}}  ; live
contract Liveness {
    uint256 a;
    uint256 b;
    mapping(address => uint256) m;

    function overwritten(uint256 x) external {
        a = 1;
        b = x;
        a = 2;
    }

    function acrossCallee() external {
        a = 1;
        readB();
        a = 2;
    }

    function readByCallee() external {
        a = 1;
        readA();
        a = 2;
    }

    function beforeRevert(uint256 x) external {
        a = x;
        revert();
    }

    function acrossExternalCall(address target) external {
        m[msg.sender] = 1;
        (bool ok, ) = target.call("");
        ok;
        m[msg.sender] = 2;
    }

    function readA() internal view returns (uint256) { return a; }

    function readB() internal view returns (uint256) { return b; }
}
