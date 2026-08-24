//@ revisions: homestead byzantium
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
// ported-from: test/libsolidity/syntaxTests/functionCalls/staticcall_on_homestead.sol
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_accessing_returned_dynamic_array_without_returndata_support.sol

interface Target {
    function dynamicReturn() external returns (bytes memory);
}

contract C {
    function calls(Target target, address addr) external {
        addr.staticcall("");
        //~[homestead]^ ERROR: builtin `staticcall` requires Byzantium-compatible EVM

        bytes memory externalData = target.dynamicReturn();
        //~[homestead]^ ERROR: mismatched types

        (, bytes memory lowLevelData) = addr.call("");
        //~[homestead]^ ERROR: mismatched types

        target.dynamicReturn();
    }

    function catches(Target target) external {
        try target.dynamicReturn() returns (bytes memory) {
        //~[homestead]^ ERROR: mismatched types
        } catch Error(string memory) {
        //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
        } catch Panic(uint256) {
        //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
        } catch (bytes memory) {
        //~[homestead]^ ERROR: typed catch clause requires Byzantium-compatible EVM
        } catch {
        }
    }
}
