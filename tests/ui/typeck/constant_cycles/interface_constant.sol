// solc reports variables in interfaces alongside constant cycles, so neither hides the other.
interface I {
    uint constant X = X; //~ ERROR: the value of the constant `X` has a cyclic dependency via `X`
    //~^ ERROR: variables cannot be declared in interfaces
}
