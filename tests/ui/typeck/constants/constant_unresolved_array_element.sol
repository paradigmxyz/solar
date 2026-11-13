// An array constant whose element type does not resolve reports the name, not the constant's type.
contract C {
    Unknown[2] constant A; //~ ERROR: unresolved symbol `Unknown`
    //~^ ERROR: constant variable must be initialized
}
