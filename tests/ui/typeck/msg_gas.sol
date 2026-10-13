contract C {
    function f() public view returns (uint256 val) { return msg.gas; } //~ WARN: `msg.gas` is deprecated
}
