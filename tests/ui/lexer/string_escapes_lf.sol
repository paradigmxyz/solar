// Escaped - OK
string constant s = "\
";
string constant s = unicode"\
";
bytes constant s = hex"\
";
//~^^ ERROR invalid hex digit
//~| ERROR invalid hex digit
// 2 for \\, \n

// Escaped, but can only escape one newline
string constant s = "\

"; //~^ ERROR cannot skip multiple lines
string constant s = unicode"\

"; //~^ ERROR cannot skip multiple lines
bytes constant s = hex"\

";
//~^^^ ERROR invalid hex digit
//~| ERROR invalid hex digit
//~^^^^ ERROR invalid hex digit
// 3x for \\, \n, \n

// Unescaped
string constant s = "
"; //~^ ERROR unescaped newline
string constant s = unicode"
"; //~^ ERROR unescaped newline
bytes constant s = hex"
";
//~^^ ERROR invalid hex digit
// 1x for \n
