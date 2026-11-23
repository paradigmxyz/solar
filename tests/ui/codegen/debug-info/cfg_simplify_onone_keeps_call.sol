//@ compile-flags: -Onone --emit=srcmap-runtime -Zdump=evm-ir-runtime
//@ run-call: init 0x0000000000000000000000000000000000000001
//@ run-call-fail: init 0x0000000000000000000000000000000000000000 => 0xd92e233d
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// A call that ends a function is a tail call, and its jump carries the call's location, so the call
// statement owns code that runs. The `require` in front of it keeps its conditional jump to the
// block that makes the call.
abstract contract Base {
    error ZeroAddress();
    address public a;

    // CHECK-LABEL: ; function __Base_init(
    // CHECK: push [[CALL:bb[0-9]+]] !metadata(span=[[#REQUIRE:]]..{{[0-9]+}})
    // CHECK-NEXT: jumpi !metadata(span=[[#REQUIRE]]..{{[0-9]+}})
    // CHECK: [[CALL]]:
    // CHECK-NEXT: jump bb{{[0-9]+}} !metadata(span=[[#SET:]]..{{[0-9]+}}
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
