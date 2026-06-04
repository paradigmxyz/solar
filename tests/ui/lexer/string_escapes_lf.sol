// Escaped - OK
string constant s = "\
";
//~v ERROR: identifier `s` already declared
string constant s = unicode"\
";
//~v ERROR: identifier `s` already declared
bytes constant s = hex"\
";
//~^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
// 2 for \\, \n

// Escaped, but can only escape one newline
//~v ERROR: identifier `s` already declared
string constant s = "\

"; //~^ ERROR: cannot skip multiple lines
string constant s = unicode"\

"; //~^ ERROR: cannot skip multiple lines
//~^^^ ERROR: identifier `s` already declared
bytes constant s = hex"\

";
//~^^^ ERROR: invalid hex digit
//~| ERROR: invalid hex digit
//~^^^^ ERROR: invalid hex digit
//~^^^^^^ ERROR: identifier `s` already declared
// 3x for \\, \n, \n

// Unescaped
//~v ERROR: identifier `s` already declared
string constant s = "
"; //~^ ERROR: unescaped newline
string constant s = unicode"
"; //~^ ERROR: unescaped newline
//~^^ ERROR: identifier `s` already declared
bytes constant s = hex"
";
//~^^ ERROR: invalid hex digit
//~^^^ ERROR: identifier `s` already declared
// 1x for \n
