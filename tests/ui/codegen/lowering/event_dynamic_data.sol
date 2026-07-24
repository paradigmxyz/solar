//@ check-pass
//@compile-flags: -Zcodegen -Zdump=mir

contract EventDynamicData {
    event Text(uint256 indexed id, string message, uint256 count);
    event Blob(bytes data);

    function text(string memory message) external {
        emit Text(1, message, 7);
    }

    function literal() external {
        emit Text(2, "solar", 9);
    }

    function blob(bytes memory data) external {
        emit Blob(data);
    }
}
