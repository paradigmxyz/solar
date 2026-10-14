// A version pragma that fails to parse is not checked against the compiler version.
pragma solidity 0.; //~ ERROR: expected version number
pragma solidity 0 - 1 0 - 2; //~ ERROR: ranges can only be combined using the || operator
