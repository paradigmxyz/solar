// Constant not allowed in error parameters
contract C {
    error E(uint constant x); //~ ERROR: mutability is not allowed here
}
