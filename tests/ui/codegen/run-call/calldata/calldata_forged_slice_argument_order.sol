//@ codegen-matrix: standard
//@ run-call-fail: structConstructor 0 => 0x
//@ run-call-fail: structConstructor 1 => 0x
//@ run-call-fail: internalCall 0 => 0x
//@ run-call-fail: internalCall 1 => 0x
//@ run-call-fail: tuple 0 => 0x
//@ run-call-fail: tuple 1 => 0x
//@ run-call-fail: stopInArgument 0 => 0x
//@ run-call: allocationSeenByArgument => 0x40

// A calldata slice that assembly moved out of range reverts with empty data when it is copied to
// memory. The copy happens as soon as the argument is evaluated, before `x - 1`, so `x == 0`
// reverts empty here; solc evaluates every argument first and reverts with `Panic(0x11)`. In
// `stopInArgument(0)` a later argument ends the frame with `stop()`: solc stops successfully and
// we revert empty. In `allocationSeenByArgument()` a later argument reads the free memory pointer
// after the copy and sees 0x40 of growth; solc sees none. See CODEGEN-033 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    struct S {
        bytes a;
        uint256 b;
    }

    function structConstructor(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        S memory m = S({a: s, b: x - 1});
        return m.b;
    }

    function internalCall(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        return take(s, x - 1);
    }

    function tuple(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        (bytes memory m, uint256 y) = (s, x - 1);
        return y + m.length;
    }

    function stopInArgument(uint256 x) external pure returns (uint256) {
        bytes calldata s;
        assembly {
            s.offset := 0xffffff
            s.length := 0
        }
        return take(s, stopIfZero(x));
    }

    function allocationSeenByArgument() external pure returns (uint256) {
        uint256 start = freePointer();
        return take(msg.data[0:4], freePointer() - start);
    }

    function freePointer() internal pure returns (uint256 p) {
        assembly {
            p := mload(0x40)
        }
    }

    function stopIfZero(uint256 x) internal pure returns (uint256) {
        assembly {
            if iszero(x) { stop() }
        }
        return x;
    }

    function take(bytes memory, uint256 y) internal pure returns (uint256) {
        return y;
    }
}
