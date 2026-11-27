// A variable or a method of another object is not the input step.
def input = readFile('input.txt')
echo input
def parsed = parser.input('x')
writeFile file: 'out.txt', text: input.trim()
