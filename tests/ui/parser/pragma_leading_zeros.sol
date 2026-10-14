// Regression test for https://github.com/paradigmxyz/solar/issues/1881.
// A number token that starts with a leading zero is illegal in a pragma, and the version is not
// checked.

pragma solidity 0.8.17 00.6.2; //~ ERROR: token incompatible with Solidity parser
pragma solidity >=00.8.0; //~ ERROR: token incompatible with Solidity parser
pragma solidity 00; //~ ERROR: token incompatible with Solidity parser
//~v ERROR: token incompatible with Solidity parser
pragma solidity >=0.8.0 /* 0 */ 01 || 00;
//~^ ERROR: token incompatible with Solidity parser

// Leading zeros after a dot do not start a number token.
pragma solidity 0.08.0; //~ WARN: source file requires different compiler version
pragma solidity 0.8.07; //~ WARN: source file requires different compiler version
pragma solidity 1.2.3.04; //~ WARN: source file requires different compiler version
pragma solidity >=0.8.00;
