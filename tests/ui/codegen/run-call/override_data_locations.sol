//@ run-call: f [9, 8] => [9, 8]
//@ run-call: g [9, 8] => [9, 8]
//@ compile-flags: --allow=2018
// ported-from: test/libsolidity/semanticTests/inheritance/dataLocation/external_public_calldata.sol

abstract contract A {
    function f(uint256[] calldata a) external virtual returns (uint256[] calldata);
}

contract B is A {
    function f(uint256[] memory a) public override returns (uint256[] memory) {
        return a;
    }

    function g(uint256[] calldata x) public returns (uint256[] memory) {
        return f(x);
    }
}
