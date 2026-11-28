pragma abicoder v1; //~ ERROR: ABI coder v1 has been removed
pragma abicoder v2;
pragma abicoder "v1"; //~ ERROR: ABI coder v1 has been removed
pragma abicoder "v2";

// These aren't accepted by solc.
pragma "abicoder" v1; //~ ERROR: ABI coder v1 has been removed
pragma "abicoder" v2;
pragma "abicoder" "v1"; //~ ERROR: ABI coder v1 has been removed
pragma "abicoder" "v2";

pragma experimental ABIEncoderV2; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma experimental "ABIEncoderV2"; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma experimental SMTChecker; //~ ERROR: the experimental SMTChecker pragma has been removed
pragma experimental "SMTChecker"; //~ ERROR: the experimental SMTChecker pragma has been removed

// These aren't accepted by solc.
pragma "experimental" ABIEncoderV2; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma "experimental" "ABIEncoderV2"; //~ ERROR: the experimental ABIEncoderV2 pragma has been removed
pragma "experimental" SMTChecker; //~ ERROR: the experimental SMTChecker pragma has been removed
pragma "experimental" "SMTChecker"; //~ ERROR: the experimental SMTChecker pragma has been removed

pragma solidity ^0.9.0.0;
pragma solidity ^0.9.0.1; //~ WARN: source file requires different compiler version
