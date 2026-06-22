//@ compile-flags: -Zcodegen --emit=mir

contract InvalidUtf8StringStorage {
    string s = "\xa0\x00"; //~ ERROR: mismatched types
}

contract LocalStoragePointer {
    string s;

    function f() public {
        string storage ref = s;
        ref = "abc"; //~ ERROR: mismatched types
    }
}
