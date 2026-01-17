// Events and functions can shadow `this` and `super` builtins.
// https://github.com/paradigmxyz/solar/issues/216

contract C {
    event this();
    event super();

    function f() public view returns (address) {
        // The `this` expression should still work.
        return address(this);
    }
}
