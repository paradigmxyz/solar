// Regression test for https://github.com/paradigmxyz/solar/issues/1843.
// The unescape error span must not be relative to the sub-denomination.
t["ei
"wei //~^ ERROR: sub-denominations are only allowed on number and rational literals
//~^^ ERROR: unescaped newline
//~^^ ERROR: expected one of
