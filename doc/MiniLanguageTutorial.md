# Mini language tutorial

[version of June 5th, 2020]

Mini is a programming language and compiler designed for writing code for the Arbitrum Virtual Machine (AVM) platform.  The language is simple but has some features customized for AVM.  This tutorial will tell you what you need to know to write Mini programs.

[This document lists potential changes or improvements to Mini in brackets like this.  Your feedback on these ideas is welcome.]

## Structure of a program

A Mini program is written as a set of source code files.  You can write separate source code files, and compile and link them together to make an executable AVM program.

## Top level declarations

A Mini source code file consists of a series of top-level declarations.  Import declarations declare types and functions that live in other files and are being imported so that the code in the current file can use them. Non-import declarations are everything else. 

The import declarations must come first. It is an error to put an import declaration after a non-import declaration.

Within each group (import or non-import) the order of declarations does not matter.

### Import declarations

`import` type *name*;

> This declares an imported type, which is assumed to be defined in another source code file.  The code in the local file can refer to the type as *name*, but it cannot know anything about the internals of the type.  The only operations that can be done on imported types are operations that are valid for any type.

`import [impure] func` *name* ( *argname1: type1, argname2: type2, ...* ) *returntype*;

> This declares an imported function, which is assumed to be defined in another source code file. The code in the local file can call this function as if it were in the local file. (The linker will generate an error if this function is called in the local file but is not provided by another file that is being linked.) The syntax follows function declaration syntax (as defined below), except that an import ends with a semicolon where a local function declaration would instead have the function's code.
>
> The optional `impure` modifier specifies that the function is impure, as defined below.
>
> The compiler does not check whether the type signature declared here matches the type signature of the actual implementation of the function elsewhere.  If the type signatures are different, this might lead to a runtime error.
>
> [Potential change: we could add the word unsafe to this somehow. Then we could say that every construction that can lead to an undetected type error has the word `unsafe` in its name.]
>
> [Potential improvement: allow interfaces to be specified in separate files, and allow a code file to say that it implements or uses a particular interface. This would make it easier to maintain consistency, and easier for the compiler to check consistency..]

### Non-import declarations

`type` *name* = *type*

> This declares a type alias, allowing name to be used as a synonym for the specified type.  (Note that in the current syntax there is not a semicolon at the end.)

var *name* : *type* ;

> This declares a global variable. If type is an atomic type, the variable will be initialized to the zero value for that type. Otherwise the variable will be uninitialized. Reading an uninitialized variable before initializing it will cause undefined behavior.

`[public] [impure] func` *name* ( *argname1: type1, argname2: type2, ...* ) [-> *returntype] codeblock*

`[public] [impure] func` *name* ( *argname1: type1, argname2: type2, ...* ) `noreturn` codeblock*

> This declares a function and provides its code.
>
> The `public` modifier is optional.  It indicates that the function can be called by code outside this source code file. Non-public functions cannot be called directly by outside code.  (However, pointers to non-public functions can be passed to outside code, and this would allow the pointed-to function to be called by outside code.)
>
> The `impure` modifier is optional. It indicates that the function is impure, meaning that it might access global variables or call other impure functions.
>
> The arguments are treated as local variables within the function, so code in the function can read them or assign to them.
>
> If there is a `returntype`, the function will return a single value of the specified type. (We'll see below that the type can be a tuple, allowing multiple values to be packaged together into a single return value.)
>
> If there is a `returntype`, the compiler must be able to infer that execution cannot reach the end of *codeblock* (so that the function terminates via a `return` statement, or the function runs forever). If the compiler is unable to verify this, it will generate an error.
>
> If `returntype` is `every`, then the function cannot return, because no return value can exist. The compiler will verify that the function cannot return. If the compiler is unable to verify this, it will generate an error.
>
> Declaring a function as `noreturn` is equivalent to declaring that the function returns `every`.  

## Types

Mini is a type-checked language.  The compiler should catch any inconsistent use of types. We believe there are only two ways that type errors can go undetected by the compiler: (1) `import func` statements that use a different type signature from the actual implementation of the function, and (2) incorrect uses of the `unsafecast` operator.

Mini has the following types:

`bool`

> true or false (an atomic type with the zero value `false`)

`uint`

> a 256-bit unsigned big-endian integer (an atomic type with the zero value `0`, and a numeric type)

`int`

> a 256-bit signed (twos complement) big-endian integer (an atomic type with the zero value `0`, and a numeric type)

`bytes32`

> a string of 32 bytes (an atomic type with the zero value of 32 zero bytes)

`address`

> a 20-byte Ethereum address (an atomic type with zero value of 0)

( *type1*, *type2*, ... )

> a tuple, consisting of zero or more ordered, typed fields (a compound type)

[ *size* ] *type*

> a fixed-size array of values, all of the same type; *size* must be a constant nonzero unsigned integer (a compound type)

[ ] *type*

> an array of values, all of the same type (a compound type)

`map` < *type* , *type* >

> a hash map, which maps keys of one type to values of another type

`union<` *type1*, *type2*, ... `>`

> an untagged union type that can hold values from any of *type1*, *type2*, etc. There must be explicit casts to convert to and from its component types. This is done via `newunion` and `unioncast`.

`struct` { *name1: type1 , name2 : type2 , ...* }

> a struct with one or more named, typed fields (a compound type)

`option`< *type* >

> either the contained type or "None<*type*>", must be unwrapped to compare with inner type

[`impure`] `func` ( *type1, type2, ...*) [-> *returntype*]

> a reference to a function

`any`

> a value of unknown type

`every`

> a type that has no values, so that no value of this type can ever exist

## Equality and assignability for types

Two types are equal if they have the same structure. Type aliases, as defined by non-import declarations, do not create a new type but simply define a shorthand method for referring to the underlying type.  (For example, after the declaration "`type foo = uint`", foo and uint are the same type.)

Every atomic type is equal to itself.

Two tuple types are equal if they have the same number of fields and their field types are equal, field-by-field.

Two fixed-size array types are equal if they have the same size and their field types are equal.

Two array types are equal if their field types are equal.

Two struct types are equal if have the same number of fields, and each field has the same name and equal type, field-by-field.

Two func types are equal if they are both impure or both not-impure, and they have the same number of argument types, and each argument type is equal, argument-by-argument, and the return types are equal (or neither has a return type).

Two map types are equal if their key types are equal and their value types are equal.

Two option types are equal if their inner types are equal

`any` equals itself.

`every` equals itself.

Each imported type equals itself.

Unless specified as equal by the rules above, a pair of types is unequal.

### Assignability

A value of type `V` is assignable to storage of type `S` if:

* `S` is `any`, or
* `V` equals `S`,
* `V` and `S` are tuple types with the same number of fields, and each field of `V` is assignable to the corresponding field of `S`,
* `V` and `S` are fixed-size arrays of the same size, and the field type of `V` is assignable to the field type of `S`,
* `V` and `S` are arrays, and the field type of `V` is assignable to the field type of `S`,
* `V` and `S` are structs, with the same number of fields, and each field of `V` has the same name as the corresponding field of `S`, and each field of `V` is assignable to the corresponding field of `S`,
* `V` and `S` are function types, with the same number of arguments, and either `S` is impure or `V` is not impure, and each argument type of `V` is assignable to the corresponding argument type of `S`, and either (a) both `S` and `V` return void, or (b) the return type of `S` is assignable to the return type of `V`.  (Note that the return type is compared for assignability "backwards". This is needed to make calls through function references type-safe.)
* `V` and `S` are map types, and the key type of `V` is assignable to the key type of `S`, and the value types of `V` and `S` are equal.
* `V` and `S` are optional types, and the inner type of `V` is assignable to the inner type of `S`

These rules guarantee that assignability is transitive. 

The compiler uses often uses type inference to infer the types of variables from the types of values assigned to them.  If a programmer wants the compiler to infer a different type, they should use an explicit type-casting operation to convert the value to the desired type.

## Castability

A value of type `V` is castable to storage of type `S` if:

* `S` is `any`, or
* `V` equals `S`,
* `V` is `bool` and `S` is one of `bool`, `address`, `bytes32`, `uint`, or `int`
* `V` is `adress` and `S` is one of `address`, `bytes32`, `uint`, or `int`
* `V` is one of `bytes32`, `uint`, or `int` and `S` is one of `bytes32`, `uint`, or `int`
* `V` and `S` are tuple types with the same number of fields, and each field of `V` is castable to the corresponding field of `S`,
* `V` and `S` are fixed-size arrays of the same size, and the field type of `V` is castable to the field type of `S`,
* `V` and `S` are arrays, and the field type of `V` is castable to the field type of `S`,
* `V` and `S` are structs, with the same number of fields, and each field of `V` is castable to the corresponding field of `S`,
* `V` and `S` are function types, with the same number of arguments, and either `S` is impure or `V` is not impure, and each argument type of `V` is castable to the corresponding argument type of `S`, and either (a) both `S` and `V` return void, or (b) the return type of `S` is castable to the return type of `V`.  (Note that the return type is compared for castability "backwards". This is needed to make calls through function references type-safe.)
* `V` and `S` are map types, and the key type of `V` is castable to the key type of `S`, and the value type of `V` is castable to the value type of `S`.
* `V` and `S` are optional types, and the inner type of `V` is castable to the inner type of `S`

## Covariant Cast

A value of type `V` can be `covariantcast` to storage of type `S` if 
there is a constructable type `T` such that `T`
is castable to both `V` and `S`.

A constructable type is a type for which it is possible to create a value.
Non-constructable types include `void`, `every` and `(uint, every)`.

In practical terms this means:

A value of type `V` is `covariantcast`able to storage of type `S` if:

* `V` is any, or
* `S` is `any`,
* `V` equals `S`,
* both `V` and `S` are one of `bool`, `address`, `bytes32`, `uint`, or `int`
* `V` and `S` are tuple types with the same number of fields, and each field of `V` is covariant castable to the corresponding field of `S`,
* `V` and `S` are fixed-size arrays of the same size, and the field type of `V` is covariant castable to the field type of `S`,
* `V` and `S` are arrays, and the field type of `V` is covariant castable to the field type of `S`,
* `V` and `S` are structs, with the same number of fields, and each field of `V` is covariant castable to the corresponding field of `S`,
* `V` and `S` are function types, with the same number of arguments, and each argument type of `V` is covariant castable to the corresponding argument type of `S`, and either (a) both `S` and `V` return void, or (b) the return type of `S` is covariant castable to the return type of `V`.
* `V` and `S` are map types, and the key type of `V` is covariant castable to the key type of `S`, and the value types of `V` and `S` are covariant castable.
* `V` and `S` are optional types

## Values

All values in Mini are immutable. There is no way to modify a value. You can only create new values that are equal to existing ones with modifications.  (That's what the `with` operator does.)

Because values are immutable, there is no notion of a reference to a value.  As far as the semantics of Mini are concerned, there are only values, and any assignment or passing of values is done by copying (although the compiler might optimize by copying a pointer rather than copying the object).

[Implementation note: Because of immutability, the compiler can choose whether to implement "copying" of an object by creating a fresh copy of its contents or by just creating a new pointer reference to the object. The difference only affects the efficiency of the generated code.  Currently, the underlying AVM emulator copies a value if its type is atomic, and copies a reference to it otherwise.  Because of immutability, it is impossible to create a cyclic data structure. This means that the underlying implementation doesn't need to use garbage collection but can always use reference-counting to achieve perfect cleanup of unreachable copy-by-reference objects.]

### Comparing values for equality

Two values are equal if they have the same type and the same contents.  Equality checking for compound types works as expected, with a "deep comparison" of the fields.  Two function references are equal if they refer to the same function.  

Values of type `anytype` do not have any representation that is understood by the compiler. Two `anytype` values will be equal if they have the same representation in the underlying AVM architecture.  So it could be the case that if values of two different types are constructed, and both are assigned to variables of type `anytype`, the resulting values could test as equal.  (Details of data representations are not described here.) Caution is advised before comparing `anytype` values.

[Potential improvement: prohibit equality comparison of anytypes.]

## Codeblocks

`{` *statement* * `}`

> A codeblock is a sequence of zero or more statements, enclosed in curly braces.  Codeblocks form the bodies of functions, `if` statements, and loops.  Local variables may be declared and used within a codeblock. A local variable that is declared within a codeblock can be used only within that same codeblock (or other codeblocks nested inside of it).

## Statements

*codeblock*

> The codeblock is executed, any variables declared inside the codeblock will only be accessible inside the codeblock.

`loop` *codeblock*

> An "infinite loop" which executes codeblock repeatedly. The only way to exit the loop is via a `return` statement (or a panic).

`while` ( *condition* ) *codeblock*

> Like a loop, except *condition*  (which must be an expression of type `bool`) is evaluated before each iteration, and the loop terminates if *condition* is found to be false.

`if` ( *condition* ) *codeblock*

`if` ( *condition* ) *codeblock* `else` *codeblock*

`if` ( *condition* ) *codeblock* `elseif` ( *condition* ) *codeblock*

`if` ( *condition* ) *codeblock* `elseif` ( *condition* ) *codeblock* `else` *codeblock*

> If statements, with the expected behavior.  You can string together as many elseifs as you want.

`let` *name* = *expression* ;

> Create a new local variable and initialize it with the value of *expression*.  The compiler infers that the new variable has the same type as *expression* .  The variable goes out of scope when execution leaves the current codeblock.  If the new variable has the same name as an already-existing variable, it will mask the existing variable definition for as long as the new variable is in scope.

`let` ( *nameorbinding1* , *nameorbinding2*, ... ) = *expression* ;

> Where each nameorbinding may be:

*identifier*

> Or

**identifier*

> Creates or assigns to multiple variables based on unpacking a tuple. If the *nameorbinding* is an identifier it creates a new variable, if *nameorbinding* is **identifier* it assigns to an existing variable with that name.  *expression* must be a tuple type, with the number of fields in the tuple equal to the number of names on the left-hand side.  The compiler creates a new local variable for each name on the left-hand side, and infers the type of each new variable based on the type of the corresponding field of the right-hand side tuple.
>
> [Potential improvement: Allow left-hand side names to be replaced by `_`, allowing unneeded components to be discarded without creating a variable.]
>
> [Potential improvement: Allow assignment directly into existing variables, or a mix of new and existing variables, rather than requiring creation of new variables.  I would have done this already but couldn't figure out a clean syntax for it--suggestions are welcome.]
>
> [Potential improvement: This could become a more general pattern-matching assignment mechanism.  Currently it pattern-matches only for a one-level tuple.]

`if let` Some(*nameLeft*) = *nameRight* *codeblock* [else *elseblock*]

> if *nameRight* is the Some variant of an option type, a new local variable *nameLeft* is created with the inner value of *nameRight* inside *codeblock*, and *codeblock* is run.  If *nameRight* is the None variant and *elseblock* is present, then *elseblock* is run instead.  *nameRight* must always be an option type.

*expression*

> The expression is executed, this is primarily useful when the side effects of *expresssion* are desired, but the either the expression does not return a value or the value is not needed.

*name* = *expression* ;

> Assign a new value to an existing variable, which can be a global variable or a local variable. The type of *expression* must be assignable to the variable's type.

`set` *name* ( .*field1*, .*field2*, ... ) = *expression* ;

> Assign to an existing variable a copy of its current value with the indicated field set to *expression*. The type of *expression* must be assignable to the field's type.

`set` *name* [ *indexExpression* ] = *expression;

> Assign to an existing variable a copy of its current value with the indicated array field value for index *indexExpression* set to *expression*. The access is bounds-checked, and will panic at runtime if the index is outside the bounds of the array. The type of *expression* must be assignable within the array.

`set` *name* [ *keyExpression* ] = *expression;

> Assign to an existing variable a copy of its current value with the indicated map field value for key *keyExpression* set to *expression*. The key is added rather than modified if it did not exist in the original map. The type of *expression* must be assignable within the map.

`return` ;

> Return from the current function. This is an error if the function has a *returntype*.

`return` *expression* ;

> Return a value from the current function. The value of *expression* must be assignable to the function's *returntype*.

`return` `None` `;`

> In a function that returns *option\<type\>*, this returns *None\<type\>*.

`asm` (*expression1*, *expression2*, ... )  { *instructions* } ;

> Escape to assembly code.  The arguments (*expression1*, *expression2*, etc.), if any, are pushed onto the AVM stack (with *expression1* at the top of the stack). Then the *instructions*, which are a sequence of AVM assembly instructions, are executed.  The assembly instructions are assumed to consume the arguments and leave nothing on the stack.  (There is another form of `asm`, which is an expression and returns a value on the stack.)

`panic` ;

> Generate an error condition.  The precise effect of this depends on the underlying AVM architecture.

;

> A null statement which does nothing.

## Expressions

There are many types of expressions, which we'll catalog here.   Operator precedence works roughly as expected.  Generally the operators listed earlier here have higher precedence.

Mini never automatically converts types to make an operation succeed.  Programmers will need to do explicit conversions. The compiler will report an error if a conversion would be necessary.

\- *expression*

> Unary minus. Defined only for type `int`, and produces an `int`. This will panic if the result is not expressible in the `int` datatype (that is, if the value of expression is `MaxNegInt`).

! *expression*

> Logical negation, defined only for type `bool`, produces a `bool`.

~ *expression*

> Bitwise negation.  Defined for numeric types and `bytes32`. Produces a result of the same type as the operand.

*expression* ?

> The containing function must return either option or any, and *expression* must be an option. If expression is the Some variant, then it evaluates as the inner value, otherwise, this will cause the function to return None.

*expression* + *expression*

*expression* - *expression*

> Addition and subtraction.  Both operands must have the same numeric type, and the result is of that same type. These do 256-bit arithmetic and do not check for overflow or underflow.

*expression* * *expression*

*expression* / *expression*

*expression* % *expression*

> Multiplication, integer division, and modulo. Both operands must have the same numeric type, and the result is of that same type. Multiplication does 256-bit arithmetic and does not check for overflow or underflow.  Division and modulo panic if the second operand is zero.

*expression* < *expression*

*expression* > *expression*

*expression* <= *expression*

*expression* >= *expression*

> Numeric comparisons. Both operands must have the same numeric type. The result has type `bool`.

*expression* == *expression*

*expression* != *expression*

> Equality comparison, under the rules for equality comparison of values as described above. The result has type `bool`.

*expression* & *expression*

*expression* ^ *expression*

*expression* | *expression*

> Bitwise and, xor, and or.  Both operands must have the same atomic type, and the result will have that same type.

*expression* && *expression*

*expression* || *expression*

> Logical and / or.  Both operands must have type `bool`, and the result has type `bool`.  Execution will shortcut, so that the second expression is evaluated only if the outcome is still in doubt after evaluating the first expression.

`uint`( *expression* )

`int`( *expression* )

`bytes32`( *expression* )

`address` ( *expression* )

> Type conversions. The operand must be an atomic type. The result type is per the operator name.  Conversion to address truncates the operand value to 20 (lowest-order) bytes.

`len` ( *expression* ) 

> Get the length of *expression*, whose value must be a non-fixed size array.  Result is a `uint`.

`hash` ( *expression* )

`hash` ( *expression* , *expression* )

> Compute the hash of the value(s).  The single-argument version can take a value of any type.  The two-argument version requires both arguments to be `bytes32`. Both produce a `bytes32`.
>
> [Improvement needed: Currently the one-argument hash  just hashes the underlying AVM representation of the value. This will cause some values of different types to have equal hashes.  It seems better to guarantee that two values test as equal (using ==) if and only if they have the same hash. To do that, we would need to generate code that incorporates a typecode of some sort into the hash.]
>
> [Possible improvement: For some user-defined data structures, the "representation hash" approach we use here won't make sense.  We might approach this by adding a "nohash" modifier to types.  Attempts to hash an object whose type had the nohash modifier would generate an error. Or possibly we want a "nocompare" modifier which would prohibit both equality comparisons and hashing.  Note that the underlying implementation uses hashing to do comparisons of non-atomic types, so values are comparable if and only if they are hashable.]
>
> [Likely improvement: Eliminate the two-argument hash, on the rationale that the programmer can always make a tuple and hash that using the single-argument hash. Applying the single-argument hash to general values is a simpler and equally expressive mechanism.]

`struct` { *name1* : *expression1* , *name2* : *expression2* , ... }

> Create a new struct value. The types of the struct fields are inferred from the types of the expressions. Returns a struct value whose type is determined by the sequence of names and expression types given.

( *expression1* , *expression2*, ... )

> Create a new tuple value, whose type will be inferred from the number and types of the expressions.

`newarray` < *type* > ( *expression* )

> Create a new array object. *expression*, which must have type `uint`, gives the size of the array, and *type* is the type of its elements. The contents of the array are initialized to the zero value if *type* is an atomic type, or uninitialized otherwise.

`newfixedarray` ( *size* )

> Create a new fixed-array of `anytype` elements. *size*, which must be a `uint` constant, is the size of the new array.

`newfixedarray` ( *size* , *expression* )

> Create a new fixed-size array of elements, with every slot initialized to the value of *expression*. *size*, which must be a `uint` constant, is the size of the new array. The element type is inferred from the type of *expression*.

`newmap` < *type* ,  *type* > 

> Create a new map object, initially empty.

`unioncast<` *type* `>(` *expression* `)`

> Converts from a type of `union<`*type1*, *type2*,...`>` to *type*, where *type* must be a member of *type1*, *type2*,.... This is an unsafe operation, as which type the union contains is not checked. 

`unsafecast` < *type* > ( *expression*  )

> Evaluate *expression*, and then treat the in-memory representation of the result as an object having type *type*. This is an unsafe operation.  It is most often used to convert a value of type `any` into a more specific type, when the programmer knows the real type of the value.  

`Some` (*expression*)

> Creates an optional with inner value equal to the result of *expression*

`None`<*type*>

> Creates an optional value of type option<*type*> with no inner value 

`newunion<` *type1*, *type2*, ... `>(` *expression* `)`

> Creates a value of type `union<*type1*, *type2*, ... >` from an *expression* of any of *type1* *type2*

*arrExpression* [ *indexExpression* ]

> Get an element of an array.  *arrExpression* must have type [ ]T or [N]T for some type T.  *indexExpression* must have type `uint`.  The access is bounds-checked, and this will panic at runtime if the index is outside the bounds of the array. The result has type T.

*mapExpression* [ *keyExpression* ]

> Get a value from a map.  mapExpression must be a map type. keyExpression, which must be assignable to the map's key type, gives the key to look up in the map. The result, which is of type (V, bool) where V is the value type of the map, will be (undefined, false) is there is not a value associated with the key, or (value, true) if value is associated with the key.

*expression* . *name*

> Access a field of a struct.  The type of *expression* must be a struct that has a field called *name*. The result has the type of that field.

*expression* . *number*

> Access a field of a tuple.  *number*, which must be a constant `uint`, specifies which field number to access.  (The first field is number zero.) *expression* must be a tuple type with more than *number* fields. The result has the type of that field.

*funcExpression* ( *argExpression1* , *argExpression2* , ... )

> Function call.  The value of *funcExpression* must be a function reference. (Typically *funcExpression* will just be the name of a function.) The number of *argExpressions* must be consistent with the number of arguments in *funcExpression*'s type, and each *argExpression* must be assignable to the type of the corresponding argument of *funcExpression*.  The result has the type of *funcExpression's* return value. (Calls to functions without a returntype are statements, not expressions.)

*arrayExpression* with { [ *indexExpression* ] = *valExpression* }

> Create a new array by copying an existing array with one element modified.  *arrayExpression*, which must be an array type, specifies the array to start with. *indexExpression*, which must have type `uint`, specifies which slot in the array should be modified.  *valExpression*, whose type must be assignable to the element type of the array, is the new value to put into the slot.  The result has the same type as *arrayExpression*. If the index is out of bounds, this will cause either a compile-time error or a runtime panic.  

*mapExpression* `with` { [ *keyExpression* ] = *valExpression* }

> Create a new map by copying an existing map with one element added or modified. *mapExpression*, which must be a map type, specifies the map to start with. *keyExpression*, which must be assignable to the map's key type, specifies the key to be added or modified. *valExpression*, which must match the map's value type, is the new value to be associated with the key.  The result has the same type as *mapExpression*.

*structExpression* with { *name* : *valExpression* }

> Create a new struct by copying an existing struct with one field modified.  *structExpression*, which must be a struct type with a field called *name*, specifies the struct to start with.  *valExpression*, which must have a type assignable to the named field, is the value that will be assigned to the named field in the newly created struct. 

`false`

`true`

`null`

> Literal values. `false` and `true` are of type bool, and `null` is the empty tuple ( ).

*number*

> An integer constant, in decimal format (or hexadecimal format, if it starts with "0x"). This will be interpreted as a `uint`, and it must be representable as a `uint`. If a decimal number is followed by the single character 's', it is interpreted as a signed integer `int`; in this case it must be representable as an `int`.

*name*

> A reference to a local variable, a global variable, or a function. It will have the type of the referenced variable or function.

`asm` ( *expression1* , *expression2* , ... ) *type* { *instructions* }

> Escape to assembly code.  The arguments (*expression1*, *expression2*, etc.), if any, are pushed onto the AVM stack (with *expression1* at the top of the stack). Then the *instructions*, which are a sequence of AVM assembly instructions, are executed.  The assembly instructions are assumed to consume the arguments and leave on the stack a single value of type *type*, which becomes the result of this expression. (There is another form of `asm`, which produces no result value and is a statement.)



