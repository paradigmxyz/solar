//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

interface IERC20Minimal {
    function transfer(address to, uint256 value) external returns (bool);
}

contract LowLevelCallReturndata {
    function safeTransfer(address token, address to, uint256 value) public {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Minimal.transfer.selector, to, value));
        require(success && (data.length == 0 || abi.decode(data, (bool))), "TF");
    }

    function balanceOf(address token) public view returns (uint256) {
        (bool success, bytes memory data) =
            token.staticcall(abi.encodeWithSignature("balanceOf(address)", address(this)));
        require(success);
        return abi.decode(data, (uint256));
    }

    function forward(address target, bytes memory payload) public returns (bytes memory) {
        (bool success, bytes memory result) = target.call(payload);
        require(success);
        return result;
    }
}
