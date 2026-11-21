-- String method lookup: indexing a string reads the `string` table.
local s = "Hello"
assert(("abc"):upper() == "ABC")
assert(s:sub(1, 2) == "He")
assert(s:sub(-3) == "llo")
assert(s:len() == 5)
assert(s:lower() == "hello")
assert(s:reverse() == "olleH")
assert(s:byte(1) == 72)
assert(("").len == string.len)
assert(s.sub == string.sub)
assert(s.nothing == nil)
assert(("x").upper ~= nil)

-- chained calls
assert(s:upper():lower():reverse() == "olleh")
assert(("abc"):upper():sub(2, 3) == "BC")

-- `#` and concatenation are unchanged
assert(#s == 5)
assert(#"" == 0)
assert(s .. " world" == "Hello world")
assert(1 .. "a" == "1a")

-- methods added to the string table become methods
function string.shout(self) return self:upper() .. "!" end
assert(("hey"):shout() == "HEY!")
string.shout = nil
assert(("hey").shout == nil)

-- the method receives the string as its first argument
local function check_self(self, n) return self, n end
string.probe = check_self
local a, b = s:probe(7)
assert(a == "Hello" and b == 7)
string.probe = nil

-- method call on a string held in a table field and in an upvalue
local t = { name = "table" }
assert(t.name:upper() == "TABLE")
local function up() return s:upper() end
assert(up() == "HELLO")

-- non-string values still cannot be indexed
assert(not pcall(function() return (5).x end))
assert(not pcall(function() return (nil).x end))

-- numbers are not strings: no method lookup
assert(not pcall(function() return (1):upper() end))
