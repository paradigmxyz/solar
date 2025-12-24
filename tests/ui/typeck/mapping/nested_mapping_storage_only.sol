//@compile-flags: -Ztypeck

contract C {
    struct S { mapping(uint => uint) m; }

    // These should error - mappings in memory/calldata (use internal to avoid public function check)
    function f1(S memory s) internal {} //~ ERROR: only valid in storage because it contains a (nested) mapping
    function f3() internal {
        S memory s; //~ ERROR: only valid in storage because it contains a (nested) mapping
    }

    // These should be OK - mappings in storage
    S storageVar;
    function f4(S storage s) internal {}
    function f5() internal {
        S storage s = storageVar;
    }
}
