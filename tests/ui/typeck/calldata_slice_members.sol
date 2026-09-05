contract C {
    function f(bytes calldata data, uint256 start, uint256 end) external pure returns (uint256) {
        return data[start:end].length;
    }

    function g(uint256[] calldata a) external pure returns (uint256) {
        return a[1:].length + a[1:][0];
    }

    function h(string calldata s) external pure returns (uint256) {
        return s[1:].length; //~ ERROR: member `length` not found on type `string calldata slice`
    }

    function i(bytes calldata data) external pure {
        data[1:].length = 1; //~ ERROR: expression has to be an lvalue
    }
}
