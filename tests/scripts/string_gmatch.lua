local function collect(s, p, init)
  local out = {}
  for a, b in string.gmatch(s, p, init) do
    out[#out + 1] = tostring(a) .. (b ~= nil and ("|" .. tostring(b)) or "")
  end
  return out
end

local function same(t, ...)
  local want = { ... }
  assert(#t == #want, "count " .. #t .. " vs " .. #want .. ": " .. table.concat(t, ","))
  for i = 1, #want do
    assert(t[i] == want[i], "item " .. i .. ": " .. t[i] .. " vs " .. want[i])
  end
end

assert(type(string.gmatch("a", "a")) == "function")

-- words and pairs
same(collect("one two three", "%a+"), "one", "two", "three")
same(collect("a=1, b=2", "(%w+)=(%w+)"), "a|1", "b|2")
same(collect("hello world", "o"), "o", "o")
same(collect("hello", "x"))
same(collect("", "a"))
same(collect("abc", "."), "a", "b", "c")
same(collect("a1b22c333", "%d+"), "1", "22", "333")
same(collect("k1=v1;k2=v2", "(%w+)=(%w+)"), "k1|v1", "k2|v2")

-- empty matches
same(collect("abc", ""), "", "", "", "")
same(collect("abc", "x*"), "", "", "", "")
same(collect("", ""), "")
same(collect("a,b,,c", "([^,]*)"), "a", "b", "", "c")
same(collect("abc", "()"), "1", "2", "3", "4")
same(collect("abc", "b*"), "", "b", "")

-- init
same(collect("abcabc", "b", 3), "b")
same(collect("abcabc", "b", 5), "b")
same(collect("abcabc", "b", 6))
same(collect("abc", ".", 10))
same(collect("abc", ".", 4))
same(collect("abc", ".", -1), "c")
same(collect("abc", ".", -2), "b", "c")
same(collect("abc", ".", 0), "a", "b", "c")
same(collect("abc", ".", -100), "a", "b", "c")

-- balanced and frontier
same(collect("(a)(b(c))", "%b()"), "(a)", "(b(c))")
same(collect("THE (quick) fox", "%f[%a]%a+"), "THE", "quick", "fox")
same(collect("x=1,y=22", "%f[%w]%w+"), "x", "1", "y", "22")

-- anchors other than ^ work inside the subject
same(collect("ab\nab", "b$"), "b")

-- back-references
same(collect("aa bb cd", "(%w)%1"), "a", "b")

-- iterators are independent
local it1 = string.gmatch("a b c", "%a")
local it2 = string.gmatch("a b c", "%a")
assert(it1() == "a")
assert(it1() == "b")
assert(it2() == "a")
assert(it1() == "c")
assert(it2() == "b")
assert(it1() == nil)
assert(it2() == "c")
assert(it2() == nil)

-- exhausted iterators keep returning nil
local it = string.gmatch("x", "x")
assert(it() == "x")
assert(it() == nil)
assert(it() == nil)

-- a loop ends cleanly on the first nil
local n = 0
for _ in string.gmatch("aaaa", "a") do n = n + 1 end
assert(n == 4)

-- multiple captures come back together
local it = string.gmatch("k=v", "(%w)=(%w)")
local a, b = it()
assert(a == "k" and b == "v")
assert(select("#", it()) == 1)

-- method form
local n = 0
for w in ("a b c"):gmatch("%a") do n = n + 1 end
assert(n == 3)

-- pattern errors surface when the iterator runs
local ok = pcall(function()
  for _ in string.gmatch("abc", "[") do end
end)
assert(not ok)
local ok = pcall(function()
  for _ in string.gmatch("abc", "(") do end
end)
assert(not ok)
assert(not pcall(string.gmatch))
assert(not pcall(string.gmatch, "a"))

-- long inputs
local big = string.rep("ab ", 20000)
local count = 0
for w in string.gmatch(big, "%a+") do count = count + 1 end
assert(count == 20000)
