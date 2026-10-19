pipeline {
    agent any
    foo {
        bar 'baz'
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
