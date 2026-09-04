// Finding 44: five try/catch statement checks solc applies are missing. solc rejects each
// function below with the code in the comment; we compile the file.
//   solc --bin symbolic-audit/try_catch_clause_checks.sol
//   target/debug/solar --emit abi symbolic-audit/try_catch_clause_checks.sol
interface I { function f() external returns (uint256); }
contract C {
    function g() internal returns (uint256) { return 1; }
    // solc 2536: try can only be used with external function calls and contract creation
    function internalCallee() external returns (uint256 r) {
        try g() returns (uint256 v) { r = v; } catch { r = 7; }
    }
    // solc 1036: this try statement already has an "Error" catch clause
    function twoErrorClauses(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 v) { r = v; } catch Error(string memory) { r = 1; } catch Error(string memory) { r = 2; }
    }
    // solc 5320: this try statement already has a low-level catch clause
    function twoLowLevelClauses(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 v) { r = v; } catch (bytes memory) { r = 1; } catch { r = 2; }
    }
    // solc 1271: expected `catch Panic(uint ...) { ... }`
    function panicWrongType(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 v) { r = v; } catch Panic(uint8) { r = 1; }
    }
    // solc 2943: expected `catch Error(string memory ...) { ... }`
    function errorWrongType(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 v) { r = v; } catch Error(uint256) { r = 1; }
    }
}
