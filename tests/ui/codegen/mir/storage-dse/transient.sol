//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: overwritten 5 =>
//@ run-call: reentrant 7 => 7
//@ run-call: clearedAfterCall 7 => 7
//@ run-call: afterReturn true => 1
//@ run-call: afterReturn false => 2
//@ run-call-fail: dropped 5
contract TransientDse {
    uint256 transient t;
    uint256 transient u;

    // The first write is overwritten before anything reads it.
    function overwritten(uint256 x) external {
        t = x;
        t = x + 1;
    }

    // A re-entrant call reads the first write, so both stay.
    function reentrant(uint256 x) external returns (uint256) {
        t = x;
        uint256 seen = this.read();
        t = x + 1;
        return seen;
    }

    // The call may read `t` and `u` stays independent of it.
    function clearedAfterCall(uint256 x) external returns (uint256) {
        t = x;
        u = 1;
        uint256 seen = this.read();
        u = 2;
        t = 0;
        return seen;
    }

    // A return commits the transient write on the path that skips the overwrite.
    function afterReturn(bool early) external returns (uint256) {
        u = 1;
        if (early) {
            return this.readU();
        }
        u = 2;
        return this.readU();
    }

    // A revert discards the transient write.
    function dropped(uint256 x) external {
        t = x;
        revert();
    }

    function read() external view returns (uint256) {
        return t;
    }

    function readU() external view returns (uint256) {
        return u;
    }
}
