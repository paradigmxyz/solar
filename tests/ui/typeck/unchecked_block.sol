contract C {
    function a() public {
        unchecked {}

        unchecked {
            unchecked {} //~ ERROR: "unchecked" blocks cannot be nested.
        }
    }
}
