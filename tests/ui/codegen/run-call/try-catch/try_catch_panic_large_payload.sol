//@ codegen-matrix: standard
//@ run-call: panicCode(); gas=3500000 => 17
//@ run-call: panicOrAnything(); gas=3500000 => 17
//@ run-call: bare(); gas=3500000 => 2

// `catch Panic(uint256)` and a bare `catch` read only the selector and the code, so a callee that
// reverts with a large Panic payload does not make the caller copy it.
contract C {
    function bomb() external pure {
        assembly {
            mstore(0, shl(224, 0x4e487b71))
            mstore(4, 0x11)
            revert(0, 0x100000)
        }
    }

    function panicCode() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch Panic(uint256 code) {
            return code;
        }
    }

    function panicOrAnything() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch Panic(uint256 code) {
            return code;
        } catch {
            return 3;
        }
    }

    function bare() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch {
            return 2;
        }
    }
}
