// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/420_interface_variables.sol

interface I {
    uint a; //~ ERROR: variables cannot be declared in interfaces
}

// Every state variable is rejected, whatever its visibility or mutability, and the getter of a
// public variable is not reported as an interface function of its own.
interface J {
    uint256 internal_; //~ ERROR: variables cannot be declared in interfaces
    uint256 public x; //~ ERROR: variables cannot be declared in interfaces
    uint256 constant CST = 1; //~ ERROR: variables cannot be declared in interfaces
    uint256 immutable IMM = 2; //~ ERROR: variables cannot be declared in interfaces
}

// Only variable declarations are rejected: parameters, return parameters, struct fields and
// event or error parameters are all allowed.
interface K {
    struct S {
        uint256 a;
    }
    event E(uint256 indexed a);
    error Err(uint256 a);
    function f(S memory s, uint256 a) external returns (uint256 b);
}

// The restriction only applies to interfaces.
contract C {
    uint256 x;
    uint256 constant Y = 1;
}
