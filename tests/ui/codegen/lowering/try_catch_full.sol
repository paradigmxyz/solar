//@run-call: dynamicOverload() => 3
//@run-call: createSuccess() => 1
//@run-call: createFailure() => 7
//@run-call: malformedErrorFallsThrough() => 36
//@run-call: unpaddedErrorAccepted() => 7
//@run-call: createDynamicArg() => 3

// Full try/catch shape: return bindings decode the successful call's
// returndata; catch clauses dispatch on the revert selector — `Error(string)`
// on 0x08c379a0, `Panic(uint256)` on 0x4e487b71 with a length check, a
// low-level `catch (bytes)` takes the rest — and with no applicable handler
// the revert data is rethrown unchanged. Verified behaviorally against solc.

interface ICallee {
    function f(uint256 a) external pure returns (uint256, string memory);
}

contract TryChild {
    constructor(bool fail) {
        require(!fail, "failed");
    }
}

contract TryDynamicChild {
    bytes public data;

    constructor(bytes memory value) {
        data = value;
    }

    function length() external view returns (uint256) {
        return data.length;
    }
}

contract TryCatchFull {
    ICallee internal c;

    // TCF-LABEL: fn @full
    // Success path decodes both returns from returndata.
    // TCF: call
    // TCF: returndatacopy
    // Error handler matches the selector.
    // TCF: 0x8c379a0
    // Panic handler checks the selector and payload length.
    // TCF: 0x4e487b71
    function full(uint256 a) public view returns (uint256 kind, bytes memory info) {
        try c.f(a) returns (uint256 v, string memory s) {
            return (v, bytes(s));
        } catch Error(string memory reason) {
            return (1, bytes(reason));
        } catch Panic(uint256 code) {
            return (2, abi.encode(code));
        } catch (bytes memory data) {
            return (3, data);
        }
    }

    // TCF-LABEL: fn @rethrows
    // An unmatched revert forwards the data unchanged.
    // TCF: returndatacopy
    // TCF: revert
    function rethrows(uint256 a) public view returns (uint256 r) {
        try c.f(a) returns (uint256 v, string memory) {
            r = v;
        } catch Error(string memory) {
            r = 1;
        }
    }

    function echo(bytes calldata data) external pure returns (uint256) {
        return data.length;
    }

    function echo(uint256) external pure returns (uint256) {
        return 99;
    }

    function dynamicOverload() external view returns (uint256) {
        try this.echo(hex"010203") returns (uint256 value) {
            return value;
        } catch {
            return 100;
        }
    }

    function createSuccess() external returns (uint256) {
        try new TryChild(false) returns (TryChild child) {
            return address(child) == address(0) ? 0 : 1;
        } catch {
            return 2;
        }
    }

    function createFailure() external returns (uint256) {
        try new TryChild(true) returns (TryChild) {
            return 1;
        } catch Error(string memory reason) {
            return bytes(reason).length + 1;
        }
    }

    function revertMalformedError() external pure {
        assembly {
            mstore(0, shl(224, 0x08c379a0))
            mstore(4, 0)
            revert(0, 36)
        }
    }

    function malformedErrorFallsThrough() external view returns (uint256) {
        try this.revertMalformedError() {
            return 0;
        } catch Error(string memory) {
            return 1;
        } catch (bytes memory data) {
            return data.length;
        }
    }

    function revertUnpaddedError() external pure {
        assembly {
            mstore(0, shl(224, 0x08c379a0))
            mstore(4, 32)
            mstore(36, 7)
            mstore(68, "abcdefg")
            revert(0, 75)
        }
    }

    function unpaddedErrorAccepted() external view returns (uint256) {
        try this.revertUnpaddedError() {
            return 0;
        } catch Error(string memory reason) {
            return bytes(reason).length;
        } catch {
            return 99;
        }
    }

    function createDynamicArg() external returns (uint256) {
        TryDynamicChild child = new TryDynamicChild(hex"010203");
        return child.length();
    }
}
