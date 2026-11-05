// Like solc, an array constant with a zero length reports both the length and the constant's
// type, at any depth.
contract C {
    uint[0] constant A = A; //~ ERROR: array length must be greater than zero
    //~^ ERROR: only constants of value type and byte array type are implemented
    uint[0][2] constant B = B; //~ ERROR: array length must be greater than zero
    //~^ ERROR: only constants of value type and byte array type are implemented
    uint[2][0] constant D = D; //~ ERROR: array length must be greater than zero
    //~^ ERROR: only constants of value type and byte array type are implemented
}
