//@ revisions: mir runtime
//@[mir] compile-flags: -O gas -Zdump=mir
//@[mir] filecheck:
//@[runtime] compile-flags: -O gas
//@ run-call: vested; constructor=[11] => 11

contract PublicBody {
    uint256 private immutable _start;

    constructor(uint256 start_) {
        _start = start_;
    }

    function start() public view returns (uint256) {
        return _start;
    }

    // CHECK-LABEL: fn @vested{{[( ]}}
    // CHECK: loadimmutable _start
    // CHECK-NOT: icall @start
    function vested() public view returns (uint256) {
        return start();
    }
}
