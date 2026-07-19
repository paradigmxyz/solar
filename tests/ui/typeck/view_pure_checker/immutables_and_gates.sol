abstract contract A {
    uint256 immutable literal = 1;
    uint256 immutable expression = 1 + 2;
    uint256 immutable runtime;

    constructor(uint256 value) {
        runtime = value;
    }

    function literals() public pure returns (uint256) {
        return literal + expression;
    }

    function runtimeValue() public pure returns (uint256) {
        return runtime;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function unimplemented() public view virtual returns (uint256);

    function implementedVirtual() public view virtual returns (uint256) {
        return 1;
    }
}
