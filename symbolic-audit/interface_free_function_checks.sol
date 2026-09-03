// Finding 35: two declaration checks solc applies are missing. solc rejects both declarations;
// we compile the file.
//   solc --bin symbolic-audit/interface_free_function_checks.sol
//   target/debug/solar --emit bin symbolic-audit/interface_free_function_checks.sol
interface I {
    function f() public;
    function g() internal;
}
function free() virtual {}
