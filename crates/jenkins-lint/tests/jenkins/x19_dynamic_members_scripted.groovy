def name = 'X'
env.(name) = 'v'
println 'a'.(name)()
def nothing = null
println nothing?.(name)
println(['a']*.(name))
def pointer = 'a'.&(name)
println new Object().@(name)
println env.(name + '_Y')
println env
    .(name)
env.(name)++
def handlers = [{ x -> x }]
handlers[0] 'x'
handlers[0] x: 1
handlers[0] 'x' { }
handlers[0] { }
def map = [k: { v -> v }]
map['k'] 'v'
this.(name) 'a' b 'c'
this.(name) { }
def a = 1
a.{ 2 }
a?.{ 2 }
println a.{ it }
