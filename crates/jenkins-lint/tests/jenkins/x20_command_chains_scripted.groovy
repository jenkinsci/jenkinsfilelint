node {
    dut run --log-level debug
    foo a b -c d
    foo bar 1
    "x" 1
    "x" foo, bar
    "x" a: 1
    def a = [1, , 2]
    def m = [a: 1, , b: 2]
    f(1, , 2)
    java.lang.String[] names = []
    def retimage
    (retimage, image) = [1, 2]
    def s = "a \
b"
}
