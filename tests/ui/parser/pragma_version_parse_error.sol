// Pragmas that fail to parse report only the parse error, not a version mismatch.

pragma solidity 0.; //~ ERROR: expected version number
pragma solidity 88_; //~ ERROR: unexpected trailing characters
pragma solidity 0 - 1 0 - 2; //~ ERROR: ranges can only be combined using the || operator
pragma solidity ^99.99.0; //~ WARN: source file requires different compiler version
