function f() {
    f; //~ WARN: statement has no effect
    f ();
    f ({ });

    revert; //~ ERROR: no matching declarations found
    revert ();
    revert ({ }); //~ ERROR: no matching declarations found
}
