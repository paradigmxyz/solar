// ported-from: test/libsolidity/natspecJSON/dev_return_name_no_description.sol
contract A {
    /// @return a
    function g(int x) public pure virtual returns (int a) { return 2; }
}

contract B is A {
    function g(int x) public pure override returns (int b) { return 2; }
}
