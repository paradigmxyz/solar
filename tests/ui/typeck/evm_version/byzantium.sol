//@ revisions: homestead byzantium
//@[homestead] compile-flags: --evm-version homestead
//@[byzantium] compile-flags: --evm-version byzantium
// ported-from: test/libsolidity/syntaxTests/functionCalls/staticcall_on_homestead.sol

interface Target {
    function value() external returns (uint256);
}

contract C {
    function calls(address addr) external view {
        addr.staticcall("");
        //~[homestead]^ ERROR: builtin `staticcall` requires Byzantium-compatible EVM
    }

    function catches(Target target) external {
        try target.value() returns (uint256) {
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
