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

// Only the escaped newline is removed.
string constant s3 = "\

"; //~^ ERROR: unescaped newline
string constant s4 = unicode"\

"; //~^ ERROR: unescaped newline
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
