function f() {
    f;
    f ();
    f ({ });

    revert;
    revert ();
    revert ({ });
}
