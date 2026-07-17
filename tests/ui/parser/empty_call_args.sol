function f() {
    f;
    f ();
    f ({ });

    revert; //~ ERROR: no matching declarations found
    revert ();
    revert ({ }); //~ ERROR: no matching declarations found
}
