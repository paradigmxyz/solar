contract StdAssertions {
    event log_array(uint256[] val);
    event log_array(int256[] val);
    event log_array(address[] val);
    event log_named_array(string key, uint256[] val);
    event log_named_array(string key, int256[] val);
    event log_named_array(string key, address[] val);
}

contract Test is StdAssertions {
    function test_doStuff() public {
        uint[] memory x;
        emit log_array(x);
        emit log_named_array("name", x);

        emit StdAssertions.log_array(x);
        emit StdAssertions.log_named_array("name", x);
    }
}
