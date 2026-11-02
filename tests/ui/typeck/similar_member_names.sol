struct S {
    uint256 total;
    uint256 a;
}

contract C {
    S s;

    function getValue() public pure returns (uint256) {
        return 1;
    }

    function f(address addr) public view {
        s.totl; //~ ERROR: member `totl` not found
        s.b; //~ ERROR: member `b` not found
        addr.balanse; //~ ERROR: member `balanse` not found
        msg.sendr; //~ ERROR: member `sendr` not found
        this.getVlaue(); //~ ERROR: member `getVlaue` not found
        this.unrelated(); //~ ERROR: member `unrelated` not found
    }
}
