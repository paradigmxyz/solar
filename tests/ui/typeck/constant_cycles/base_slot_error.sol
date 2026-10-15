// An error in a base slot does not hide the cycles of other constants.
uint constant C0 = 1;
uint constant C1 = C0 + 1;
uint constant C2 = C1 + 1;
uint constant C3 = C2 + 1;
uint constant C4 = C3 + 1;
uint constant C5 = C4 + 1;
uint constant C6 = C5 + 1;
uint constant C7 = C6 + 1;
uint constant C8 = C7 + 1;
uint constant C9 = C8 + 1;
uint constant C10 = C9 + 1;
uint constant C11 = C10 + 1;
uint constant C12 = C11 + 1;
uint constant C13 = C12 + 1;
uint constant C14 = C13 + 1;
uint constant C15 = C14 + 1;
uint constant C16 = C15 + 1;
uint constant C17 = C16 + 1;
uint constant C18 = C17 + 1;
uint constant C19 = C18 + 1;
uint constant C20 = C19 + 1;
uint constant C21 = C20 + 1;
uint constant C22 = C21 + 1;
uint constant C23 = C22 + 1;
uint constant C24 = C23 + 1;
uint constant C25 = C24 + 1;
uint constant C26 = C25 + 1;
uint constant C27 = C26 + 1;
uint constant C28 = C27 + 1;
uint constant C29 = C28 + 1;
uint constant C30 = C29 + 1;
uint constant C31 = C30 + 1;
uint constant C32 = C31 + 1;
uint constant Z = Z + 1; //~ ERROR: the value of the constant `Z` has a cyclic dependency via `Z`

contract X layout at C32 {} //~ ERROR: failed to evaluate constant: constants are nested too deeply
