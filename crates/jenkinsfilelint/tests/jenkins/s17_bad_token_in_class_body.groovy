class Foo {
    def bar() { return ) }
}
pipeline {
    agent any
    stages { stage('S') { steps { echo 'x' } } }
}
