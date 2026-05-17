function f(uint256) pure {}

using {f} for *; //~ ERROR: the type has to be specified explicitly at file level
