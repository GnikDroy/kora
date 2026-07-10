# Grammar

```ebnf
module      = { import | struct | impl | extern | function } ;

import      = "import" STRING [ ident ] ";" ;

struct      = "struct" ident "{" [ member { "," member } [ "," ] ] "}" ;
member      = ident ":" type ;

impl        = "impl" ident "{" { method } "}" ;
method      = rettype ident "(" "self" [ "," [ param { "," param } [ "," ] ] ] ")" block ;

extern      = "extern" rettype ident "(" params ")" ";" ;
function    = rettype ident "(" params ")" block ;
rettype     = "void" | type ;
params      = [ param { "," param } [ "," ] ] ;
param       = ident ":" type ;

type        = basetype { "?" } ;                (* "?" makes it optional *)
basetype    = "int" | "real" | "char" | "bool" | "string"
            | ident                      (* struct name *)
            | "[" type "]" ;

statement   = ";"
            | expr ";"
            | "let" ident [ ":" type ] "=" expr ";"
            | "return" [ expr ] ";"
            | "break" ";"
            | "continue" ";"
            | "if" "(" expr ")" statement [ "else" statement ]
            | "while" "(" expr ")" statement
            | "for" "(" forinit expr ";" expr ")" statement
            | block ;
forinit     = ";" | expr ";" | "let" ident [ ":" type ] "=" expr ";" ;
block       = "{" { statement } "}" ;

expr        = assign ;
assign      = or [ "=" assign ] ;
or          = and  { "||" and } ;
and         = eq   { "&&" eq } ;
eq          = rel  { ( "==" | "!=" ) rel } ;
rel         = add  { ( "<" | ">" | "<=" | ">=" ) add } ;
add         = mul  { ( "+" | "-" | "|" | "^" ) mul } ;
mul         = cast { ( "*" | "/" | "%" | "&" | "<<" | ">>" ) cast } ;
cast        = unary { "as" type } ;
unary       = ( "!" | "-" ) unary | postfix ;
postfix     = primary { "(" args ")" | "[" expr "]" | "." ident | "!" } ;
args        = [ expr { "," expr } [ "," ] ] ;
primary     = INT | REAL | CHAR | STRING | "true" | "false" | "none"
            | ident
            | "(" expr ")"
            | "[" [ expr { "," expr } [ "," ] ] "]"
            | "new" type [ "[" expr "]"
                         | "{" [ field { "," field } [ "," ] ] "}" ] ;
field       = ident ":" expr ;
```
