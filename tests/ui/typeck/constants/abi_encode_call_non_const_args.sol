// ported-from: test/libsolidity/syntaxTests/constants/initialization/abi_encode_call_non_const_args.sol
contract A {
    function f(uint a) external {}

    function getA() private view returns(uint) {
        return 1;
    }

    bytes constant fCallA = abi.encodeCall(A.f, (getA())); //~ ERROR: initial value for constant variable has to be compile-time constant
}
