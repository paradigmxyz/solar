//@ run-call: f() => true

contract ExternalFunctionPointerAddress {
    function g() external {}

    function f() external view returns (bool) {
        function() external fp = this.g;
        return fp.address == address(this);
    }
}
