using {f} for * global; //~ ERROR: can only globally attach functions to specific types
//~^ ERROR: the type has to be specified explicitly at file level (cannot use `*`)
//~| ERROR: expected function name
function f(uint) pure {} //~ ERROR: function with same name and parameter types declared twice

contract C {
    using {f} for uint global; //~ ERROR: `global` can only be used at file level
    //~^ ERROR: expected function name
    //~| ERROR: can only use `global` with user-defined types
}
function f(uint) pure {}

interface I2 {
    using L for int; //~ ERROR: the `using for` directive is not allowed inside interfaces
    //~^ ERROR: unresolved symbol `L`
    function g() external;
}
