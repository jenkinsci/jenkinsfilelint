pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    foo(1) bar
                    foo(1) bar baz
                    foo(1) bar { echo 'x' }
                }
            }
        }
    }
}
