//@ codegen-matrix: standard
//@ run-call: panicCode(); gas=3500000 => 17
//@ run-call: panicOrAnything(); gas=3500000 => 17
//@ run-call: bare(); gas=3500000 => 2
//@ run-call: panicBeforeError(); gas=3500000 => 17
//@ run-call: panicBeforeBytes(); gas=3500000 => 17
//@ run-call: panicWithError(); gas=3500000 => 17
//@ run-call: pointerDelta() => 0

// `catch Panic(uint256)` and a bare `catch` read only the selector and the code, so a callee that
// reverts with a large Panic payload does not make the caller copy it, even when a sibling clause
// copies the payload on its own path.
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

    function panicBeforeError() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch Error(string memory) {
            return 2;
        } catch Panic(uint256 code) {
            return code;
        }
    }

    function panicBeforeBytes() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch Panic(uint256 code) {
            return code;
        } catch (bytes memory) {
            return 3;
        }
    }

    function panicWithError() external view returns (uint256) {
        try this.bomb() {
            return 1;
        } catch Panic(uint256 code) {
            return code;
        } catch Error(string memory) {
            return 2;
        } catch (bytes memory) {
            return 3;
        }
    }

    function small() external pure {
        assert(false);
    }

    function pointerDelta() external view returns (uint256) {
        uint256 beforePtr;
        assembly {
            beforePtr := mload(0x40)
        }
        try this.small() {
            return 999;
        } catch Panic(uint256) {
            uint256 afterPtr;
            assembly {
                afterPtr := mload(0x40)
            }
            return afterPtr - beforePtr;
        }
    }
}
