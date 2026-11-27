pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                a b c
            }
        }
    }
}
