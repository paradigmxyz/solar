contract Child {}

contract Code {
    function code() external pure returns (bytes memory) {
        return type(Child).creationCode;
    }
}
