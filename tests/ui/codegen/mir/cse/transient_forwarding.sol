//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: stored 7 => 7
//@ run-call: storedTwice 7 => 8
//@ run-call: afterCall 7 => 0
//@ run-call: otherSlot 7 => 1
contract TransientForwarding {
    uint256 transient t;
    uint256 transient u;

    // The load forwards the stored word.
    function stored(uint256 x) external returns (uint256) {
        t = x;
        return t;
    }

    // The load forwards the latest stored word.
    function storedTwice(uint256 x) external returns (uint256) {
        t = x;
        t = x + 1;
        return t;
    }

    // The call may rewrite `t`, so the load stays.
    function afterCall(uint256 x) external returns (uint256) {
        t = x;
        this.clear();
        return t;
    }

    // A write to another slot leaves the forwarded word alone.
    function otherSlot(uint256 x) external returns (uint256) {
        t = x;
        u = 1;
        return u + t - x;
    }

    function clear() external {
        t = 0;
    }
}
