//@ codegen-matrix: standard
//@ run-call: ExternalFunctionAbi::run => 1

contract ExternalFunctionAbi {
    struct Holder {
        function (uint256) external returns (uint256) fn;
        uint256 tag;
    }

    function target(uint256 value) external pure returns (uint256) {
        return value + 1;
    }

    function echo(function (uint256) external returns (uint256) fn)
        external
        pure
        returns (function (uint256) external returns (uint256))
    {
        return fn;
    }

    function echoHolder(Holder memory holder) external pure returns (Holder memory) {
        return holder;
    }

    function echoArray(function (uint256) external returns (uint256)[] memory values)
        external
        pure
        returns (function (uint256) external returns (uint256)[] memory)
    {
        return values;
    }

    function run() external returns (uint256) {
        function (uint256) external returns (uint256) original = this.target;
        function (uint256) external returns (uint256) echoed = this.echo(original);
        require(echoed.address == address(this), "address");
        require(echoed.selector == this.target.selector, "selector");

        bytes memory encoded = abi.encode(original);
        uint256 word;
        assembly {
            word := mload(add(encoded, 0x20))
        }
        require(uint64(word) == 0, "padding");
        bytes memory dirty = abi.encodeWithSelector(this.echo.selector, original);
        assembly {
            mstore8(add(dirty, 67), 1)
        }
        (bool accepted,) = address(this).call(dirty);
        require(!accepted, "dirty padding");

        Holder memory holder = Holder({fn: original, tag: 7});
        Holder memory roundTrip = this.echoHolder(holder);
        require(roundTrip.fn.address == address(this), "holder address");
        require(roundTrip.fn.selector == this.target.selector, "holder selector");
        require(roundTrip.tag == 7, "holder tag");

        function (uint256) external returns (uint256)[] memory values =
            new function (uint256) external returns (uint256)[](1);
        values[0] = original;
        function (uint256) external returns (uint256)[] memory arrayRoundTrip =
            this.echoArray(values);
        require(arrayRoundTrip[0].address == address(this), "array address");
        require(arrayRoundTrip[0].selector == this.target.selector, "array selector");
        return 1;
    }
}
