//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=TCF

// Full try/catch shape: return bindings decode the successful call's
// returndata; catch clauses dispatch on the revert selector — `Error(string)`
// on 0x08c379a0, `Panic(uint256)` on 0x4e487b71 with a length check, a
// low-level `catch (bytes)` takes the rest — and with no applicable handler
// the revert data is rethrown unchanged. Verified behaviorally against solc.

interface ICallee {
    function f(uint256 a) external pure returns (uint256, string memory);
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
}
