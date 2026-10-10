//~? ERROR: contract `B` does not use ABI coder v2 but wants to inherit from a contract which uses types that require it
// ported-from: test/libsolidity/syntaxTests/imports/inheritance_abi_encoder_mismatch_2.sol
pragma abicoder v1;
import "./auxiliary/inheritance_mismatch_b_v1.sol";
contract C is B { } //~ ERROR: contract `C` does not use ABI coder v2 but wants to inherit from a contract which uses types that require it
