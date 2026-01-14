using {f} for * global; //~ ERROR: can only globally attach functions to specific types
//~^ ERROR: the type has to be specified explicitly at file level (cannot use `*`)
function f(uint) pure {}

contract C {
    using {f} for uint global; //~ ERROR: `global` can only be used at file level
}
function f(uint) pure {}

interface I2 {
    using L for int; //~ ERROR: the `using for` directive is not allowed inside interfaces
    function g() external;
}
