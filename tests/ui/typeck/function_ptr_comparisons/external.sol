//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/functionTypes/comparison_operators_for_external_functions.sol

contract C {
    function f() external {}
    function g() external {}
    function h() external pure {}
    function i() external view {}

    function compareFunctions() public returns (bool) {
        assert(
            this.f != this.g && this.f != this.h && this.f != this.i &&
            this.g != this.h && this.g != this.i && this.h != this.i &&
            this.f == this.f && this.g == this.g && this.h == this.h && this.i == this.i
        );
        return true;
    }

    function comparePointers() public returns (bool) {
        function() external fLocal = this.f;
        function() external gLocal = this.g;
        function() external pure hLocal = this.h;
        function() external view iLocal = this.i;
        assert(
            fLocal == this.f && gLocal == this.g && hLocal == this.h && iLocal == this.i &&
            fLocal != this.g && fLocal != this.h && fLocal != this.i &&
            gLocal != this.f && gLocal != this.h && gLocal != this.i &&
            hLocal != this.f && hLocal != this.g && hLocal != this.i &&
            iLocal != this.f && iLocal != this.g && iLocal != this.h
        );
        assert(fLocal == fLocal && fLocal != gLocal && fLocal != hLocal && fLocal != iLocal);
        assert(gLocal == gLocal && gLocal != hLocal && gLocal != iLocal);
        assert(hLocal == hLocal && iLocal == iLocal && hLocal != iLocal);
        return true;
    }
}
