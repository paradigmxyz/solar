// Regression test for https://github.com/paradigmxyz/solar/issues/1842.

pragma solidity "ьь"; //~ ERROR: expected version number

//~vvv ERROR: unterminated string
//~vv ERROR: expected version number
//~v ERROR: expected `;`
pragma s&".ь
