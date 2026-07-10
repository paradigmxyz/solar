//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/functionCall/inheritance/super_skip_unimplemented_in_abstract_contract.sol

contract A {
    function f() public virtual returns (uint256) {
        return 42;
    }
}
abstract contract I {
    function f() external virtual returns (uint256);
}
contract B is A, I {
    function f() public override(A, I) returns (uint256) {
        return super.f();
    }
}
