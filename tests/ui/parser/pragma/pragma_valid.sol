pragma abicoder v1; //~ ERROR: expected `pragma abicoder v2`
pragma abicoder v2;
pragma abicoder "v1"; //~ ERROR: expected `pragma abicoder v2`
pragma abicoder "v2";

// These aren't accepted by solc.
pragma "abicoder" v1; //~ ERROR: expected `pragma abicoder v2`
pragma "abicoder" v2;
pragma "abicoder" "v1"; //~ ERROR: expected `pragma abicoder v2`
pragma "abicoder" "v2";

pragma experimental ABIEncoderV2; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma experimental "ABIEncoderV2"; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma experimental SMTChecker; //~ ERROR: unsupported experimental feature name
pragma experimental "SMTChecker"; //~ ERROR: unsupported experimental feature name

// These aren't accepted by solc.
pragma "experimental" ABIEncoderV2; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma "experimental" "ABIEncoderV2"; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma "experimental" SMTChecker; //~ ERROR: unsupported experimental feature name
pragma "experimental" "SMTChecker"; //~ ERROR: unsupported experimental feature name

pragma solidity ^0.9.0.0;
pragma solidity ^0.9.0.1; //~ WARN: source file requires different compiler version
