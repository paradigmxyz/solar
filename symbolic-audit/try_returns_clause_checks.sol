// Finding 41: a `try` `returns` clause whose variable types or count do not match the callee's
// return values is accepted; solc reports 6509 and 2800.
//   solc --bin symbolic-audit/try_returns_clause_checks.sol
//   target/debug/solar --emit abi symbolic-audit/try_returns_clause_checks.sol
interface I { function f() external returns (uint256); }
contract C {
    function wrongType(address a) external returns (uint256 r) {
        try I(a).f() returns (bool x) { r = x ? 1 : 0; } catch { r = 7; }
    }
    function wrongCount(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 x, uint256 y) { r = x + y; } catch { r = 7; }
    }
}
