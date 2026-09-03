contract TryCatchDecode {
    error Custom(uint256);
    function revertRaw(bytes calldata data) external pure { assembly { calldatacopy(0, data.offset, data.length) revert(0, data.length) } }
    function ok() external pure returns (uint256) { return 1; }
    function tryError(bytes calldata data) external view returns (uint256 kind, bytes memory got) {
        try this.revertRaw(data) { return (0, ""); }
        catch Error(string memory reason) { return (1, bytes(reason)); }
        catch Panic(uint256 code) { return (2, abi.encode(code)); }
        catch (bytes memory low) { return (3, low); }
    }
    function tryErrorOnly(bytes calldata data) external view returns (uint256 kind, bytes memory got) {
        try this.revertRaw(data) { return (0, ""); }
        catch Error(string memory reason) { return (1, bytes(reason)); }
        catch { return (3, ""); }
    }
    function tryPanicOnly(bytes calldata data) external view returns (uint256 kind, uint256 code) {
        try this.revertRaw(data) { return (0, 0); }
        catch Panic(uint256 c) { return (2, c); }
        catch { return (3, 0); }
    }
    function tryLowOnly(bytes calldata data) external view returns (bytes memory) {
        try this.revertRaw(data) { return ""; }
        catch (bytes memory low) { return low; }
    }
    function tryBare(bytes calldata data) external view returns (uint256) {
        try this.revertRaw(data) { return 0; } catch { return 9; }
    }
    function tryReturns(bytes calldata data) external view returns (uint256) {
        try this.retRaw(data) returns (uint256 v) { return v; } catch { return 9; }
    }
    function retRaw(bytes calldata data) external pure returns (uint256) { assembly { calldatacopy(0, data.offset, data.length) return(0, data.length) } }
    function tryReturnsDyn(bytes calldata data) external view returns (uint256) {
        try this.retRawDyn(data) returns (bytes memory v) { return v.length; } catch { return 9; }
    }
    function retRawDyn(bytes calldata data) external pure returns (bytes memory) { assembly { calldatacopy(0, data.offset, data.length) return(0, data.length) } }
    function requireCustom(uint256 x) external pure { if (x > 1) revert Custom(x); require(x == 1, "one"); }
    function tryCustom(uint256 x) external view returns (uint256 kind, bytes memory got) {
        try this.requireCustom(x) { return (0, ""); }
        catch Error(string memory reason) { return (1, bytes(reason)); }
        catch (bytes memory low) { return (3, low); }
    }
}
