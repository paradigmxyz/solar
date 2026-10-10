pragma abicoder v1;
pragma abicoder v2; //~ ERROR: ABI coder has already been selected for this source unit
pragma abicoder "v1"; //~ ERROR: ABI coder has already been selected for this source unit
pragma abicoder "v2"; //~ ERROR: ABI coder has already been selected for this source unit

// These aren't accepted by solc.
pragma "abicoder" v1; //~ ERROR: ABI coder has already been selected for this source unit
pragma "abicoder" v2; //~ ERROR: ABI coder has already been selected for this source unit
pragma "abicoder" "v1"; //~ ERROR: ABI coder has already been selected for this source unit
pragma "abicoder" "v2"; //~ ERROR: ABI coder has already been selected for this source unit

pragma experimental ABIEncoderV2; //~ ERROR: ABI coder v1 has already been selected through `pragma abicoder v1`
pragma experimental "ABIEncoderV2"; //~ ERROR: ABI coder v1 has already been selected through `pragma abicoder v1`
pragma experimental SMTChecker;
pragma experimental "SMTChecker";

// These aren't accepted by solc.
pragma "experimental" ABIEncoderV2; //~ ERROR: ABI coder v1 has already been selected through `pragma abicoder v1`
pragma "experimental" "ABIEncoderV2"; //~ ERROR: ABI coder v1 has already been selected through `pragma abicoder v1`
pragma "experimental" SMTChecker;
pragma "experimental" "SMTChecker";

pragma solidity ^0.8.27.0;
pragma solidity ^0.8.30.1;
