// Escaped - OK
string constant s = "\
";
string constant s = unicode"\
";
bytes constant s = hex"\
";
//~^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 3x for \\, \r, \n

// Escaped, but can only escape one newline
string constant s = "\

"; //~^ ERROR: cannot skip multiple lines
string constant s = unicode"\

"; //~^ ERROR: cannot skip multiple lines
bytes constant s = hex"\

";
//~^^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~^^^^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 5x for \\, \r, \n, \r, \n

// Unescaped
string constant s = "
"; //~^ ERROR: unescaped newline
string constant s = unicode"
"; //~^ ERROR: unescaped newline
bytes constant s = hex"
";
//~^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 2x for \r, \n
