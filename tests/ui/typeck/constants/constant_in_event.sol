// Constant not allowed in event parameters
contract C {
    event E(uint constant x); //~ ERROR: mutability is not allowed here
}
