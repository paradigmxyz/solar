// Constants that no constant references share the search through their first dependency.
contract C {
    uint constant R0 = V + 1; //~ ERROR: the value of the constant `R0` has a cyclic dependency via `V`
    uint constant R1 = V; //~ ERROR: the value of the constant `R1` has a cyclic dependency via `V`
    uint constant R2 = V + W; //~ ERROR: the value of the constant `R2` has a cyclic dependency via `W`
    uint constant R3 = V * 2; //~ ERROR: the value of the constant `R3` has a cyclic dependency via `V`
    uint constant W = R1; //~ ERROR: the value of the constant `W` has a cyclic dependency via `R1`
    uint constant V = Z + 1; //~ ERROR: the value of the constant `V` has a cyclic dependency via `Z`
    uint constant Z = Z; //~ ERROR: the value of the constant `Z` has a cyclic dependency via `Z`
}
