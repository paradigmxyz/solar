// ported-from: test/libsolidity/syntaxTests/controlFlow/storageReturn/try_err.sol
contract C {
    struct S { bool f; }
    S s;
    function ext() external {}
    function f() internal returns (S storage r) //~ ERROR: this variable is of storage pointer type and can be returned
    {
        try this.ext() { }
        catch (bytes memory) { r = s; }
    }
    function g() internal returns (S storage r) //~ ERROR: this variable is of storage pointer type and can be returned
    {
        try this.ext() { r = s; }
        catch (bytes memory) { }
    }
    function h() internal returns (S storage r) //~ ERROR: this variable is of storage pointer type and can be returned
    {
        try this.ext() {}
        catch Error (string memory) { r = s; }
        catch (bytes memory) { r = s; }
    }
    function i() internal returns (S storage r)
    {
        try this.ext() { r = s; }
        catch (bytes memory) { return r; } //~ ERROR: this variable is of storage pointer type and can be accessed
        r = s;
    }
}
