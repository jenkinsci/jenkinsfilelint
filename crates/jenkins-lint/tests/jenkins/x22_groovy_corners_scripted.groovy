@Library('lib') l2
@Field LIST = []

def <T> T inNode(String label, Closure<T> body) {
    node(label) { body() }
}

static <T> Map<String, T> empty() { [:] }

def class Helper { }

@NonCPS
runIt(script, folder) {
    echo 'x'
}

foo(String s)
def a = [1]
{ echo 'attached to the list' }
println("a")println("b")
if (a) or (b) or (c) { }
def x = 1 % 60

/${x} min/
