//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: panicClause() => 0
//@ run-call: bareCatch() => 0
//@ run-call: errorClause() => 96
//@ run-call: errorClauseThenPanic() => 96
//@ run-call: errorClauseOffset() => 128
//@ run-call: bytesClausePanic() => 96
//@ run-call: bytesClauseEmpty() => 0
//@ run-call: bytesClauseShort() => 64
//@ run-call: bytesClauseError() => 160
//@ run-call: malformedErrorToBytes() => 160
//@ run-call: panicClauseToBytes() => 96

// A catch clause allocates only what it binds: `Panic(uint256)` reads its code without allocating,
// `Error(string)` allocates the decoded message, and `bytes memory` allocates the returndata, which
// stays the zero slot when it is empty.
contract C {
    function panics() external pure {
        assert(false);
    }

    function errors() external pure {
        revert("hello");
    }

    function empty() external pure {
        revert();
    }

    function short() external pure {
        assembly {
            mstore(0, shl(240, 0xabcd))
            revert(0, 2)
        }
    }

    // Selector `Error(string)` with an offset that points outside the data.
    function malformedError() external pure {
        assembly {
            mstore(0, shl(224, 0x08c379a0))
            mstore(4, 0x1000)
            mstore(36, 0)
            mstore(68, 0)
            revert(0, 100)
        }
    }

    // `Error(string)` whose message follows a padding word.
    function paddedError() external pure {
        assembly {
            mstore(0, shl(224, 0x08c379a0))
            mstore(4, 0x40)
            mstore(36, 0xff)
            mstore(68, 3)
            mstore(100, shl(232, 0x616263))
            revert(0, 132)
        }
    }

    function customError() external pure {
        assembly {
            mstore(0, shl(224, 0x12345678))
            mstore(4, 1)
            revert(0, 36)
        }
    }

    function fmp() private pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
        }
    }

    // CHECK-LABEL: fn @panicClause()
    // CHECK-NOT: returndata_bytes
    // CHECK: revert_returndata
    function panicClause() external view returns (uint256) {
        uint256 before = fmp();
        try this.panics() {
            revert();
        } catch Panic(uint256 code) {
            require(code == 1);
            return fmp() - before;
        }
    }

    function bareCatch() external view returns (uint256) {
        uint256 before = fmp();
        try this.panics() {
            revert();
        } catch {
            return fmp() - before;
        }
    }

    // CHECK-LABEL: fn @errorClause()
    // CHECK-NOT: returndata_bytes
    // CHECK: icall @try_decode_error_message
    function errorClause() external view returns (uint256) {
        uint256 before = fmp();
        try this.errors() {
            revert();
        } catch Error(string memory reason) {
            require(bytes(reason).length == 5);
            return fmp() - before;
        }
    }

    function errorClauseThenPanic() external view returns (uint256) {
        uint256 before = fmp();
        try this.errors() {
            revert();
        } catch Error(string memory reason) {
            require(bytes(reason).length == 5);
            return fmp() - before;
        } catch Panic(uint256) {
            revert();
        }
    }

    // CHECK-LABEL: fn @bytesClausePanic()
    // CHECK-NOT: returndatacopy
    // CHECK: icall returndata_bytes<>
    function errorClauseOffset() external view returns (uint256) {
        uint256 before = fmp();
        try this.paddedError() {
            revert();
        } catch Error(string memory reason) {
            require(keccak256(bytes(reason)) == keccak256("abc"));
            return fmp() - before;
        }
    }

    function bytesClausePanic() external view returns (uint256) {
        uint256 before = fmp();
        try this.panics() {
            revert();
        } catch (bytes memory data) {
            require(data.length == 36);
            return fmp() - before;
        }
    }

    function bytesClauseEmpty() external view returns (uint256) {
        uint256 before = fmp();
        try this.empty() {
            revert();
        } catch (bytes memory data) {
            require(data.length == 0);
            return fmp() - before;
        }
    }

    function bytesClauseShort() external view returns (uint256) {
        uint256 before = fmp();
        try this.short() {
            revert();
        } catch (bytes memory data) {
            require(data.length == 2);
            return fmp() - before;
        }
    }

    function bytesClauseError() external view returns (uint256) {
        uint256 before = fmp();
        try this.errors() {
            revert();
        } catch (bytes memory data) {
            require(data.length == 100);
            return fmp() - before;
        }
    }

    function malformedErrorToBytes() external view returns (uint256) {
        uint256 before = fmp();
        try this.malformedError() {
            revert();
        } catch Error(string memory) {
            revert();
        } catch (bytes memory data) {
            require(data.length == 100);
            return fmp() - before;
        }
    }

    function panicClauseToBytes() external view returns (uint256) {
        uint256 before = fmp();
        try this.customError() {
            revert();
        } catch Panic(uint256) {
            revert();
        } catch (bytes memory data) {
            require(data.length == 36);
            return fmp() - before;
        }
    }
}
