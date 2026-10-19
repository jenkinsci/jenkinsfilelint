pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                stage('inner') {
                    echo 'x'
                }
            }
        }
    }
}
