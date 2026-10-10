// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/reverting_call_virtual3.sol
contract A {
  function f() public virtual returns (uint) { g(); } //~ WARN: unnamed return variable can remain unassigned when the function is called when `B` is the most derived contract
  function g() internal virtual { revert(); }
}
contract B is A {
  function f() public override returns (uint) { A.f(); } //~ WARN: unnamed return variable can remain unassigned
  function g() internal override {}
}
