local function err(f, ...)
  local ok, m = pcall(f, ...)
  assert(not ok, "expected an error")
  return m
end

local find, match, gsub, gmatch = string.find, string.match, string.gsub, string.gmatch

-- malformed sets
assert(err(find, "a", "[") == "malformed pattern (missing ']')")
assert(err(find, "a", "[a") == "malformed pattern (missing ']')")
assert(err(find, "a", "[]") == "malformed pattern (missing ']')")
assert(err(find, "a", "[^]") == "malformed pattern (missing ']')")
assert(err(find, "a", "[a-") == "malformed pattern (missing ']')")
assert(err(find, "a", "[%") == "malformed pattern (missing ']')")
assert(err(find, "a", "[%]") == "malformed pattern (missing ']')")
assert(err(match, "a", "[a%]") == "malformed pattern (missing ']')")
assert(err(match, "a", "x*[") == "malformed pattern (missing ']')")
assert(match("]", "[]]") == "]")
assert(match("^", "[%^]") == "^")

-- malformed escapes
assert(err(find, "a", "%") == "malformed pattern (ends with '%')")
assert(err(match, "a", "a%") == "malformed pattern (ends with '%')")
assert(err(gsub, "a", "%", "x") == "malformed pattern (ends with '%')")

-- captures
assert(err(find, "a", "(") == "unfinished capture")
assert(err(find, "a", "(a") == "unfinished capture")
assert(err(match, "a", "((a)") == "unfinished capture")
assert(err(match, "a", ")") == "invalid pattern capture")
assert(err(match, "a", "a)") == "invalid pattern capture")
assert(err(match, "a", "(a))") == "invalid pattern capture")
assert(err(find, "a", "%1") == "invalid capture index %1")
assert(err(find, "a", "(a)%2") == "invalid capture index %2")
assert(err(find, "a", "(a)%0") == "invalid capture index %0")
assert(err(find, "a", "(a%1)") == "invalid capture index %1")
assert(err(match, "a", string.rep("(", 33)) == "too many captures")
assert(err(match, "a", string.rep("()", 33)) == "too many captures")

-- %b and %f
assert(err(find, "a", "%b") == "malformed pattern (missing arguments to '%b')")
assert(err(find, "a", "%b(") == "malformed pattern (missing arguments to '%b')")
assert(err(find, "a", "%f") == "missing '[' after '%f' in pattern")
assert(err(find, "a", "%fa") == "missing '[' after '%f' in pattern")
assert(err(find, "a", "%f[a") == "malformed pattern (missing ']')")

-- a bad item is only an error if the matcher reaches it
assert(find("a", "b[") == nil)
assert(find("a", "b%") == nil)
assert(find("a", "b(") == nil)

-- errors through the other entry points
assert(err(gsub, "a", "[", "x") == "malformed pattern (missing ']')")
assert(err(gsub, "a", "(", "%1") == "unfinished capture")
assert(err(gsub, "a", "a", "%1x%2") == "invalid capture index %2 in replacement string")
assert(not pcall(function() for _ in gmatch("a", "%") do end end))
assert(not pcall(function() for _ in gmatch("a", "(") do end end))

-- the errors are catchable and leave the library usable
for i = 1, 3 do
  assert(not pcall(find, "a", "["))
  assert(find("abc", "b") == 2)
end

-- pattern length is not itself a problem
local long = string.rep("a", 5000)
assert(find(long, string.rep("a", 5000)) == 1)
assert(match(long, "^" .. string.rep("a", 4999) .. "(a)$") == "a")
