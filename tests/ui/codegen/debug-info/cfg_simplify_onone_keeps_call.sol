//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime
//@ run-call: init 0x0000000000000000000000000000000000000001
//@ run-call-fail: init 0x0000000000000000000000000000000000000000 => 0xd92e233d
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// At `-O none` a call that ends a function stays a call, as in clang's `-O0`: the call pushes its
// return address and jumps with the origin of the call, so the call statement owns code that runs,
// and the `require` in front of it keeps its conditional jump to the block that makes the call.
abstract contract Base {
    error ZeroAddress();
    address public a;

    // CHECK-LABEL: ; function __Base_init(
    // CHECK: jumpi !metadata(span=[[#REQUIRE:]]..{{[0-9]+}})
    // CHECK-NOT: invoke=_set
    // CHECK: bb[[#CALL:]]:
    // CHECK-NEXT: swap 1 !metadata(span=[[#SET:]]..{{[0-9]+}})
    // CHECK-NEXT: push bb[[#RET:]] !metadata(span=[[#SET]]..{{[0-9]+}})
    // CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[#SET]]..{{[0-9]+}})
    function __Base_init(address x) internal {
        require(x != address(0), ZeroAddress());
        _set(x);
    }

    function _set(address x) internal {
        a = x;
    }
}

contract Harness is Base {
    function init(address x) external {
        __Base_init(x);
    }
}
