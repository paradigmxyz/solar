pragma abicoder v1;
pragma abicoder v2;
pragma abicoder "v1";
pragma abicoder "v2";

// These aren't accepted by solc.
pragma "abicoder" v1;
pragma "abicoder" v2;
pragma "abicoder" "v1";
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
