-- utf8.len counts characters; # counts bytes.
local plain = "hello"
local accented = "h\u{e9}llo"            -- e-acute is 2 bytes
local mixed = "a\u{20ac}\u{1f600}z"      -- 1 + 3 + 4 + 1 bytes

assert(#plain == 5 and utf8.len(plain) == 5)
assert(#accented == 6 and utf8.len(accented) == 5)
assert(#mixed == 9 and utf8.len(mixed) == 4)
assert(utf8.len("") == 0)

-- i and j are byte positions; a character counts if it starts between them.
assert(utf8.len(mixed, 2) == 3)
assert(utf8.len(mixed, 2, 4) == 1)
assert(utf8.len(mixed, 5) == 2)
assert(utf8.len(mixed, -1) == 1)
assert(utf8.len(mixed, 1, 0) == 0)
assert(utf8.len(plain, 6) == 0)          -- one past the end counts nothing
assert(utf8.len(plain, 1, -2) == 4)

-- An invalid sequence gives nil and the position of its first byte.
local n, at = utf8.len("ab\xff")
assert(n == nil and at == 3)
n, at = utf8.len("\xe2\x82")             -- a 3-byte character cut short
assert(n == nil and at == 1)
n, at = utf8.len(mixed, 3)               -- starts inside the euro sign
assert(n == nil and at == 3)
n, at = utf8.len("\xed\xa0\x80")         -- a surrogate is not a character
assert(n == nil and at == 1)
n, at = utf8.len("\xc0\x80")             -- an overlong encoding is refused
assert(n == nil and at == 1)

-- Positions out of range are errors.
assert(not pcall(utf8.len, plain, 7))
assert(not pcall(utf8.len, plain, 0))
assert(not pcall(utf8.len, plain, 1, 6))
assert(not pcall(utf8.len, plain, -6))
assert(not pcall(utf8.len, 5 == 5))

-- A number is taken as its text, as the string library does.
assert(utf8.len(12345) == 5)
