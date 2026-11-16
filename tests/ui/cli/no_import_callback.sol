//@ compile-flags: --no-import-callback --stop-after=analysis
// Like solc, imports only resolve to the files given on the command line.

import "./auxiliary/debug_output_helper.sol"; //~ ERROR: File import callback not supported

contract C {}
