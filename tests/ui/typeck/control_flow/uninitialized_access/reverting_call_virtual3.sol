// ported-from: test/libsolidity/syntaxTests/controlFlow/uninitializedAccess/reverting_call_virtual3.sol
contract A {
  function f() public virtual returns (uint) { g(); }
  function g() internal virtual { revert(); }
}
contract B is A {
  function f() public override returns (uint) { A.f(); }
  function g() internal override {}
}
