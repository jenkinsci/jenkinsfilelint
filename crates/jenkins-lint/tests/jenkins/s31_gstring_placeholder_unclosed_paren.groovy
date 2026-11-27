pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo "x ${foo(} y"
            }
        }
    }
}
