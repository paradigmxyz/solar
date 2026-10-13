// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/165_assigning_state_to_const_variable.sol
contract C {
    address constant x = msg.sender; //~ ERROR: initial value for constant variable has to be compile-time constant
}
