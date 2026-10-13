// Regression test for https://github.com/paradigmxyz/solar/issues/1842.
// Diagnostic spans inside version strings must land on char boundaries.

pragma solidity "ьь"; //~ ERROR: expected version number
//~^ ERROR: unexpected trailing characters
pragma solidity unicode"1ьa"; //~ ERROR: unexpected trailing characters
pragma solidity "1.2.3.45ь"; //~ ERROR: unexpected trailing characters

//~vvvvvv ERROR: unterminated string
//~vvvvv ERROR: expected version number
//~vvvv ERROR: unexpected trailing characters
//~vvv ERROR: expected version number
//~vv ERROR: expected version number
//~v ERROR: expected one of `;` or `||`, found `<eof>`
pragma s&".ь