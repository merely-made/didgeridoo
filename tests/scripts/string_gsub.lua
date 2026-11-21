local gsub = string.gsub

local function check(s, p, r, n, want, count)
  local got, c = gsub(s, p, r, n)
  assert(got == want, "got " .. tostring(got) .. ", want " .. want .. " for " .. tostring(p))
  assert(c == count, "count " .. tostring(c) .. ", want " .. count .. " for " .. tostring(p))
end

-- string replacement
check("hello world", "o", "0", nil, "hell0 w0rld", 2)
check("hello", "l+", "L", nil, "heLo", 1)
check("hello", "x", "y", nil, "hello", 0)
check("", "x", "y", nil, "", 0)
check("", "", "x", nil, "x", 1)
check("abc", "", "-", nil, "-a-b-c-", 4)
check("abc", "x*", "-", nil, "-a-b-c-", 4)
check("abc", "b*", "X", nil, "XaXcX", 3)
check("abc", "%w", "%0%0", nil, "aabbcc", 3)
check("aaa", "a", "", nil, "", 3)
check("a b c", " ", "", nil, "abc", 2)
check(" a  b ", "%s+", " ", nil, " a b ", 3)

-- the limit
check("aaa", "a", "b", 2, "bba", 2)
check("aaa", "a", "b", 0, "aaa", 0)
check("aaa", "a", "b", -1, "aaa", 0)
check("aaa", "a", "b", 10, "bbb", 3)
check("hello world", "%w+", "%0 %0", 1, "hello hello world", 1)
check("abc", "", "-", 2, "-a-bc", 2)

-- %0 to %9 and %%
check("hello", "l", "[%0]", nil, "he[l][l]o", 2)
check("abc", "(a)(b)", "%2%1", nil, "bac", 1)
check("abc", "b", "[%1]", nil, "a[b]c", 1)
check("a", "a", "%%", nil, "%", 1)
check("a", "a", "100%%", nil, "100%", 1)
check("abc", "()b", "%1", nil, "a2c", 1)
check("abc", "(b)", "%1%1", nil, "abbc", 1)
check("abc", "((a)(b))", "%3%2%1", nil, "baabc", 1)
check("x", "x", "%0%0%0", nil, "xxx", 1)

-- numbers as arguments and replacements
check(123, 2, 9, nil, "193", 1)
check("abc", "b", 5, nil, "a5c", 1)
check("abc", "b", 1.5, nil, "a1.5c", 1)

-- anchors
check("hello", "^h", "J", nil, "Jello", 1)
check("hhh", "^h", "J", nil, "Jhh", 1)
check("abc", "^x", "J", nil, "abc", 0)
check("abc", "$", "!", nil, "abc!", 1)
check("abc", "^", "!", nil, "!abc", 1)
check("abc", "c$", "C", nil, "abC", 1)
check("abcabc", "abc$", "X", nil, "abcX", 1)
check("a^b", "a^b", "X", nil, "X", 1)

-- sets, frontier, balanced
check("THE (quick) fox", "%f[%a]%a+", "W", nil, "W (W) W", 3)
check("f(a)(b)", "%b()", "", nil, "f", 2)
check("  trim  ", "^%s*(.-)%s*$", "%1", nil, "trim", 1)
check("a1b2", "%d", "#", nil, "a#b#", 2)
check("a1b2", "[^%d]", "#", nil, "#1#2", 2)
check("hello", ".", "%0.", nil, "h.e.l.l.o.", 5)
check("a.b", "%.", "%%", nil, "a%b", 1)

-- binary safe
check("a\0b\0c", "%z", "-", nil, "a\0b\0c", 0)
check("a\0b", "\0", "-", nil, "a-b", 1)
check("a\0b", ".", "x", nil, "xxx", 3)

-- a gsub returns exactly two values
assert(select("#", gsub("a", "a", "b")) == 2)
assert(select("#", gsub("a", "x", "b")) == 2)

-- table replacement
check("$name is $age", "%$(%w+)", { name = "Bob", age = 42 }, nil, "Bob is 42", 2)
check("a b", "%w", { a = "X" }, nil, "X b", 2)
check("a b", "%w", { a = false, b = "Y" }, nil, "a Y", 2)
check("abc", "()", { [1] = "<", [3] = ">" }, nil, "<ab>c", 4)
check("hello world", "%w+", { hello = "bye" }, nil, "bye world", 2)
local defaulted = setmetatable({}, { __index = function(_, k) return k:upper() end })
check("ab", "%w", defaulted, nil, "AB", 2)
local inherited = setmetatable({}, { __index = { a = "1" } })
check("ab", "%w", inherited, nil, "1b", 2)
-- the first capture is the key
check("k=v", "(%w)=(%w)", { k = "K", v = "V" }, nil, "K", 1)
assert(not pcall(gsub, "a", "a", { a = true }))
assert(not pcall(gsub, "a", "a", { a = {} }))
assert(not pcall(gsub, "a", "a", { a = print }))

-- function replacement
check("abc", "%w", function(c) return c:upper() end, nil, "ABC", 3)
check("abc", "%w", function(c) if c == "b" then return "B" end end, nil, "aBc", 3)
check("abc", "%w", function() return false end, nil, "abc", 3)
check("abc", "%w", function() return nil end, nil, "abc", 3)
check("k=v", "(%w+)=(%w+)", function(k, v) return v .. "=" .. k end, nil, "v=k", 1)
check("abc", "b", function(whole) return "[" .. whole .. "]" end, nil, "a[b]c", 1)
check("abc", "()b()", function(a, b) return a .. "-" .. b end, nil, "a2-3c", 1)
check("abc", "%w", function() return 7 end, nil, "777", 3)
check("abc", "%w", function(c) return c .. c end, 2, "aabbc", 2)
check("abc", "", function() return "-" end, nil, "-a-b-c-", 4)
check("abc", "b*", function(s) return "<" .. s .. ">" end, nil, "<>a<b>c<>", 3)
check("a b", "%w+", function(w) return gsub(w, "%w", "%0%0") end, nil, "aa bb", 2)
assert(not pcall(gsub, "a", "a", function() return {} end))
assert(not pcall(gsub, "a", "a", function() return true end))
assert(not pcall(gsub, "a", "a", function() error("boom") end))

-- a function sees the arguments in order and is called once per match
local seen = {}
gsub("x=1 y=2", "(%w)=(%w)", function(k, v) seen[#seen + 1] = k .. v end)
assert(#seen == 2 and seen[1] == "x1" and seen[2] == "y2")

-- errors raised inside the function reach the caller with their value
local ok, err = pcall(gsub, "a", "a", function() error("custom", 0) end)
assert(not ok and err == "custom")

-- a replacement function can yield
local co = coroutine.create(function()
  local r = gsub("abc", "%w", function(c)
    coroutine.yield(c)
    return c:upper()
  end)
  return r
end)
local ok, v = coroutine.resume(co)
assert(ok and v == "a")
ok, v = coroutine.resume(co)
assert(ok and v == "b")
ok, v = coroutine.resume(co)
assert(ok and v == "c")
ok, v = coroutine.resume(co)
assert(ok and v == "ABC")
assert(coroutine.status(co) == "dead")

-- invalid replacement strings
local function err(...)
  local ok, m = pcall(...)
  assert(not ok)
  return m
end
assert(err(gsub, "abc", "b", "%2") == "invalid capture index %2 in replacement string")
assert(err(gsub, "abc", "(b)", "%2") == "invalid capture index %2 in replacement string")
assert(err(gsub, "abc", "b", "%") == "invalid use of '%' in replacement string")
assert(err(gsub, "abc", "b", "%x") == "invalid use of '%' in replacement string")
assert(err(gsub, "abc", "b", "a%") == "invalid use of '%' in replacement string")
-- no match, no expansion, no error
assert(gsub("abc", "x", "%2") == "abc")
assert(not pcall(gsub, "abc", "b", true))
assert(not pcall(gsub, "abc", "b"))
assert(not pcall(gsub, "abc"))

-- long strings
assert(#gsub(string.rep("a", 10000), "a", "bb") == 20000)
local _, n = gsub(string.rep("ab", 5000), "a", "")
assert(n == 5000)
local big = string.rep("x ", 5000)
local _, n = gsub(big, "%a", function(c) return c end)
assert(n == 5000)

-- method form
assert(("hello"):gsub("l", "L") == "heLLo")
assert(select(2, ("hello"):gsub("l", "L")) == 2)
