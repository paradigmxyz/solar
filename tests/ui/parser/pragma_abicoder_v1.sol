pragma abicoder v1;
//~^ ERROR: ABI coder v1 is not supported
pragma abicoder "v1";
//~^ ERROR: ABI coder v1 is not supported

// These aren't accepted by solc.
pragma "abicoder" v1;
//~^ ERROR: ABI coder v1 is not supported
pragma "abicoder" "v1";
//~^ ERROR: ABI coder v1 is not supported
