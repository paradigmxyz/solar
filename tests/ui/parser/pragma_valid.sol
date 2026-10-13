pragma abicoder v1; //~ WARN: ABI coder v1 is not supported
pragma abicoder v2;
pragma abicoder "v1"; //~ WARN: ABI coder v1 is not supported
pragma abicoder "v2";

// These aren't accepted by solc.
pragma "abicoder" v1; //~ WARN: ABI coder v1 is not supported
pragma "abicoder" v2;
pragma "abicoder" "v1"; //~ WARN: ABI coder v1 is not supported
pragma "abicoder" "v2";

pragma experimental ABIEncoderV2;
pragma experimental "ABIEncoderV2";
pragma experimental SMTChecker;
pragma experimental "SMTChecker";

// These aren't accepted by solc.
pragma "experimental" ABIEncoderV2;
pragma "experimental" "ABIEncoderV2";
pragma "experimental" SMTChecker;
pragma "experimental" "SMTChecker";

pragma solidity ^0.8.27.0;
pragma solidity ^0.8.30.1;
