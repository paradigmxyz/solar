//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/smtCheckerTests/function_selector/function_selector_via_contract_name.sol

contract A {
    function f() external {}
    function g(uint256) external {}
}
contract B {
    function f() external returns (uint256) {}
    function g(uint256) external returns (uint256) {}
}
contract C {
    function test1() external pure returns (bytes4, bytes4, bytes4, bytes4) {
        return (A.f.selector, A.g.selector, B.f.selector, B.g.selector);
    }

    function test2() external pure returns (bytes4, bytes4, bytes4, bytes4) {
        A a;
        B b;
        return (a.f.selector, a.g.selector, b.f.selector, b.g.selector);
    }
}
