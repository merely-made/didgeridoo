local find = string.find

local function check(s, p, init, plain, ...)
  local r = table.pack(find(s, p, init, plain))
  local want = table.pack(...)
  assert(r.n == want.n, "result count for " .. tostring(p) .. ": " .. r.n)
  for i = 1, want.n do
    assert(r[i] == want[i], "result " .. i .. " for " .. tostring(p))
  end
end

-- plain text
check("hello world", "o", nil, nil, 5, 5)
check("hello world", "world", nil, nil, 7, 11)
check("hello", "xyz", nil, nil, nil)
check("hello", "hello", nil, nil, 1, 5)
check("hello", "hello!", nil, nil, nil)
check("", "a", nil, nil, nil)
check("", "", nil, nil, 1, 0)
check("abc", "", nil, nil, 1, 0)
check("abc", "c", nil, nil, 3, 3)

-- a failed find returns a single nil
assert(select("#", find("a", "b")) == 1)
assert(select("#", find("abc", "b")) == 2)

-- init
check("hello", "l", 4, nil, 4, 4)
check("hello", "l", 3, nil, 3, 3)
check("hello", "l", -2, nil, 4, 4)
check("hello", "l", -1, nil, nil)
check("hello", "h", 2, nil, nil)
check("hello", "h", 0, nil, 1, 1)
check("hello", "h", -100, nil, 1, 1)
check("hello", "o", 5, nil, 5, 5)
check("hello", "o", 6, nil, nil)
check("abc", "", 4, nil, 4, 3)
check("abc", "", 3, nil, 3, 2)
check("abc", "", 5, nil, nil)
check("abc", "", 100, nil, nil)
check("abc", "", -1, nil, 3, 2)
check("abc", "", -10, nil, 1, 0)
check("", "", 1, nil, 1, 0)
check("", "", 2, nil, nil)
check("abab", "ab", 2, nil, 3, 4)
check("abab", "b+", 3, nil, 4, 4)

-- plain flag
check("a.b", ".", 1, true, 2, 2)
check("a.b", ".", 1, false, 1, 1)
check("a+b", "+", 1, true, 2, 2)
check("a%b", "%", 1, true, 2, 2)
check("a[b", "[", 1, true, 2, 2)
check("a(b", "(", 1, true, 2, 2)
check("abab", "ab", 2, true, 3, 4)
check("abc", "", 2, true, 2, 1)
check("abc", "", 4, true, 4, 3)
check("abc", "", 5, true, nil)
check("abc", "bc", -2, true, 2, 3)
check("a.b", "a.b", 1, 1, 1, 3)
check("a^b$", "^b$", 1, true, 2, 4)
check("aXb", "%u", 1, true, nil)
check("a%ub", "%u", 1, true, 2, 3)

-- anchors
check("hello", "^h", nil, nil, 1, 1)
check("hello", "^e", nil, nil, nil)
check("hello", "o$", nil, nil, 5, 5)
check("hello", "l$", nil, nil, nil)
check("hello", "^hello$", nil, nil, 1, 5)
check("hello", "^$", nil, nil, nil)
check("", "^$", nil, nil, 1, 0)
check("hello", "^", 3, nil, 3, 2)
check("abc", "^b", 2, nil, 2, 2)
check("abc", "^a", 2, nil, nil)
check("abc", "$", nil, nil, 4, 3)
check("a^b", "a^b", nil, nil, 1, 3)
check("a$b", "a$b", nil, nil, 1, 3)
check("a$", "a$", nil, nil, nil)
check("a$", "a%$", nil, nil, 1, 2)
check("ba", "^a", nil, nil, nil)

-- magic characters escaped
check("a.b", "%.", nil, nil, 2, 2)
check("a%b", "%%", nil, nil, 2, 2)
check("a*b", "%*", nil, nil, 2, 2)
check("(x)", "%(x%)", nil, nil, 1, 3)
check("a-b", "%-", nil, nil, 2, 2)
check("a]b", "%]", nil, nil, 2, 2)

-- captures are returned after the indices
check("hello world", "(o)(%s)", nil, nil, 5, 6, "o", " ")
check("hello", "()ll()", nil, nil, 3, 4, 3, 5)
check("key=val", "(%w+)=(%w+)", nil, nil, 1, 7, "key", "val")
check("abc", "((a)(b))", nil, nil, 1, 2, "ab", "a", "b")
check("abc", "()", nil, nil, 1, 0, 1)
check("abc", "()", 3, nil, 3, 2, 3)
check("abc", "()", 4, nil, 4, 3, 4)

-- quantifiers
check("aaab", "a-b", nil, nil, 1, 4)
check("aaab", "a*", nil, nil, 1, 3)
check("aaab", "a+", nil, nil, 1, 3)
check("baaa", "a+", nil, nil, 2, 4)
check("abc", "b*", nil, nil, 1, 0)
check("abc", "ab?c", nil, nil, 1, 3)
check("ac", "ab?c", nil, nil, 1, 2)
check("xyz", "y-", nil, nil, 1, 0)
check("xyz", "y-z", nil, nil, 2, 3)

-- sets, classes, frontier
check("abc123", "%d", nil, nil, 4, 4)
check("abc123", "[0-9]+", nil, nil, 4, 6)
check("abc", "[%a]+", nil, nil, 1, 3)
check("abc", "%f[%a]", nil, nil, 1, 0)
check(" a", "%f[%a]", nil, nil, 2, 1)
check("a ", "%f[%A]", nil, nil, 2, 1)
check("ab", "%f[\0]", nil, nil, 3, 2)
check("THE (quick) fox", "%f[%a]%a+", 5, nil, 6, 10)
check("a\nb", "a.b", nil, nil, 1, 3)
check("a\0b", "a.b", nil, nil, 1, 3)
check("a\0b", "\0", nil, nil, 2, 2)
check("a\0b", "%z", nil, nil, nil)

-- balanced
check("f(a(b)c)d", "%b()", nil, nil, 2, 8)
check("x(", "%b()", nil, nil, nil)

-- coercion of numbers
check(12345, 34, nil, nil, 3, 4)
check("a1", 1, nil, nil, 2, 2)

-- arguments are required
assert(not pcall(find))
assert(not pcall(find, "a"))
assert(not pcall(find, nil, "a"))
assert(not pcall(find, "a", {}))
assert(not pcall(find, "a", "a", "x"))

-- method form
assert(("hello"):find("l") == 3)
local s, e, cap = ("key=val"):find("=(%w+)")
assert(s == 4 and e == 7 and cap == "val")

-- binary-safe
local bin = "\0\1\2\255\254"
check(bin, "\255", nil, nil, 4, 4)
check(bin, "[\128-\255]+", nil, nil, 4, 5)
check(bin, "\1\2", nil, nil, 2, 3)
