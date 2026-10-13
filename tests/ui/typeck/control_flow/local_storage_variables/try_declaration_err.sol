// ported-from: test/libsolidity/syntaxTests/controlFlow/localStorageVariables/try_declaration_err.sol
contract C {
    struct S { bool f; }
    S s;
    function ext() external {}
    function f() internal
    {
        S storage r;
        try this.ext() { }
        catch (bytes memory) { r = s; }
        r; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function g() internal
    {
        S storage r;
        try this.ext() { r = s; }
        catch (bytes memory) { }
        r; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function h() internal
    {
        S storage r;
        try this.ext() {}
        catch Error (string memory) { r = s; }
        catch (bytes memory) { r = s; }
        r; //~ ERROR: this variable is of storage pointer type and can be accessed
    }
    function i() internal
    {
        S storage r;
        try this.ext() { r = s; }
        catch (bytes memory) { r; } //~ ERROR: this variable is of storage pointer type and can be accessed
        r = s;
        r;
    }
}
