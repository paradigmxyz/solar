// ported-from: test/libsolidity/syntaxTests/abiEncoder/v1_inheritance_from_contract_defining_v2_function_accepting_struct.sol
pragma abicoder v1;
import "./auxiliary/v2_function_accepting_struct.sol";

contract D is C {} //~ ERROR: contract `D` does not use ABI coder v2 but wants to inherit from a contract which uses types that require it
