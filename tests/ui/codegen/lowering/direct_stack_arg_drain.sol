//@ compile-flags: -O gas
//@ run-call: g 10 => 24
//@ run-call: h 10 => 25
//@ run-call: i 10 => 30
//@ run-call: j 10 => 31

// A static-frame internal callee whose `resident` selection bails on a `Switch`
// terminator can fall through to the `direct` stack-argument convention, which
// keeps its arguments on the stack with no frame store. A stack-draining
// internal call (`rec`) between the entry stack and a later use of those
// arguments must not leave them to be reloaded from their never-written frame
// slots. Two callers keep `f` from inlining so the convention is exercised.
contract C {
    uint256 s;

    function rec(uint256 n) internal returns (uint256) {
        if (n == 0) return 0;
        return n + rec(n - 1);
    }

    function f(uint256 a, uint256 b) internal returns (uint256) {
        unchecked {
            uint256 t = a + 1;
            s = rec(s);
            uint256 r = t + a + b;
            uint256 out;
            assembly {
                switch r
                case 0 { out := 100 }
                case 1 { out := 101 }
                case 2 { out := 102 }
                default { out := r }
            }
            return out;
        }
    }

    function g(uint256 x) external returns (uint256) { return f(x, 3); }
    function h(uint256 x) external returns (uint256) { return f(x, 3) + 1; }

    // A switch drains its selector layout before rebuilding the terminator inputs. A multi-use
    // argument must therefore retain a memory home rather than use the direct-only convention.
    function switchOnly(uint256 a) internal pure returns (uint256 out) {
        uint256 twice = a + a;
        assembly {
            switch a
            case 0 { out := 1 }
            default { out := a }
        }
        return twice + out;
    }

    function i(uint256 x) external pure returns (uint256) { return switchOnly(x); }
    function j(uint256 x) external pure returns (uint256) { return switchOnly(x) + 1; }
}
