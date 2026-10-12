// Error-typed operands must not produce follow-up diagnostics.

contract C {
    mapping(M => uint) m; //~ ERROR: unresolved symbol `M`

    function f(bytes calldata cd) external {
        M[1:2]; //~ ERROR: unresolved symbol `M`
        abi.decode(cd, (M)); //~ ERROR: unresolved symbol `M`
        abi.encodeCall(M, ()); //~ ERROR: unresolved symbol `M`
        bytes.concat(M); //~ ERROR: unresolved symbol `M`
        string.concat(M); //~ ERROR: unresolved symbol `M`
        [M, 1]; //~ ERROR: unresolved symbol `M`
        [M][0]; //~ ERROR: unresolved symbol `M`
        (uint a, uint b) = M; //~ ERROR: unresolved symbol `M`
    }
}
