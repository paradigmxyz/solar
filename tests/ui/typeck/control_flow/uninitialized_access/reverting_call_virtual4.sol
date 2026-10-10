// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/reverting_call_virtual4.sol
contract A {
  function f() public virtual returns (uint) { g(); } //~ WARN: unnamed return variable can remain unassigned when the function is called when `C` is the most derived contract
  function g() internal virtual { revert(); }
}
contract B is A {
  function f() public virtual override returns (uint) { A.f(); } //~ WARN: unnamed return variable can remain unassigned when the function is called when `C` is the most derived contract
  function g() internal virtual override { A.g(); }
}
contract C is B {
  function f() public virtual override returns (uint) { A.f(); } //~ WARN: unnamed return variable can remain unassigned
  function g() internal virtual override { }
}
