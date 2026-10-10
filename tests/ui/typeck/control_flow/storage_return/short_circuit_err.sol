// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/short_circuit_err.sol
contract C {
    struct S { bool f; }
    S s;
    function f() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        false && (c = s).f;
    }
    function g() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        true || (c = s).f;
    }
    function h() internal view returns (S storage c) { //~ ERROR: this variable is of storage pointer type and can be returned
        // expect error, although this is always fine
        true && (false || (c = s).f);
    }
}
