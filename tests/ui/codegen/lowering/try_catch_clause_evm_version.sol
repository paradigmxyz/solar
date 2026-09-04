//@ revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck:

interface ClauseTarget {
    function value() external returns (uint256);
}

contract TryCatchClauses {
    // A bare `catch { }` binds nothing and compiles at every version: the return values come
    // out of the call's own output area, and the clause runs with no return data at all.
    // CHECK-LABEL: fn @bare
    // CHECK: call
    // CHECK: returndatasize
    function bare(ClauseTarget target) external returns (uint256 r) {
        try target.value() returns (uint256 v) {
            r = v;
        } catch {
            r = 7;
        }
    }

    // Every clause that matches or binds the return data needs it to exist. The type checker
    // reports that on its own, once per clause, as solc does; codegen adds nothing.
    // CHECK-LABEL: fn @typed
    function typed(ClauseTarget target) external returns (uint256 r) {
        try target.value() returns (uint256 v) {
            r = v;
        } catch Error(string memory) {
            //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
            r = 1;
        } catch Panic(uint256) {
            //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
            r = 2;
        } catch (bytes memory) {
            //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
            r = 3;
        }
    }
}
