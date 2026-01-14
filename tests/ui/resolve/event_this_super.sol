// Events named `this` and `super` are allowed and don't conflict with builtins.
// https://github.com/paradigmxyz/solar/issues/216

contract C {
    event this();
    event super();
    
    function emitEvents() public {
        emit this();
        emit super();
    }
}
