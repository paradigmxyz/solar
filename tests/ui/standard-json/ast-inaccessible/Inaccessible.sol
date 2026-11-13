contract Inaccessible {
    function dyn(uint256 x) public returns (bytes memory a) {}

    function f(uint256 x) public returns (bool) {
        this.dyn(x);
        return true;
    }
}
