//@ ignore-host: windows

// Escaped - OK
string constant s1 = "\
";
string constant s2 = unicode"\
";
bytes constant b1 = hex"\
";
//~^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 3x for \\, \r, \n

// Escaped, but can only escape one newline
string constant s3 = "\

"; //~^ ERROR: cannot skip multiple lines
string constant s4 = unicode"\

"; //~^ ERROR: cannot skip multiple lines
bytes constant b2 = hex"\

";
//~^^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~^^^^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 5x for \\, \r, \n, \r, \n

// Unescaped
string constant s5 = "
"; //~^ ERROR: unescaped newline
string constant s6 = unicode"
"; //~^ ERROR: unescaped newline
bytes constant b3 = hex"
";
//~^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 2x for \r, \n
