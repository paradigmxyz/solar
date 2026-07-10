//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/inheritance/super_overload.sol

contract A {
    function f(uint256 a) public returns (uint256) {
        return 2 * a;
    }
}
contract B {
    function f(bool) public returns (uint256) {
        return 10;
    }
}
contract C is A, B {
    function g() public returns (uint256) {
        return super.f(true);
    }

    function h() public returns (uint256) {
        return super.f(1);
    }
}
