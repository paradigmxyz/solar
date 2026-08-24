contract C {
    function index(string calldata value) external pure returns (bytes1) {
        return value[0];
        //~^ ERROR: cannot index into string calldata
    }
}
