pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                echo 'unclosed
            }
        }
    }
}
